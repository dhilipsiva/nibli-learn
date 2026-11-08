//! Isolated, revision-bound browser worker client.
#[cfg(target_arch = "wasm32")]
mod browser {
    use futures_channel::oneshot;
    use futures_util::future::{Either, select};
    use gloo_timers::future::TimeoutFuture;
    use nibli_learn_model::{Command, Reply, Request, Response, SessionView, WorkerError};
    use std::{cell::RefCell, collections::HashMap, rc::Rc};
    use wasm_bindgen::{JsCast, JsValue, closure::Closure};
    use web_sys::{ErrorEvent, MessageEvent, Worker, WorkerOptions, WorkerType};

    type Pending = oneshot::Sender<Result<Reply, WorkerError>>;
    struct Inner {
        worker: Option<Worker>,
        message: Option<Closure<dyn FnMut(MessageEvent)>>,
        error: Option<Closure<dyn FnMut(ErrorEvent)>>,
        generation: u64,
        next_id: u64,
        client_id: String,
        pending: HashMap<String, Pending>,
        session: Option<SessionView>,
    }
    impl Drop for Inner {
        fn drop(&mut self) {
            if let Some(worker) = &self.worker {
                worker.set_onmessage(None);
                worker.set_onerror(None);
                worker.terminate();
            }
        }
    }
    #[derive(Clone)]
    pub struct WorkerClient(Rc<RefCell<Inner>>);

    fn error(kind: &str, message: impl Into<String>) -> WorkerError {
        WorkerError {
            kind: kind.into(),
            message: message.into(),
            record: None,
        }
    }

    impl WorkerClient {
        pub fn new() -> Result<Self, String> {
            let crypto = web_sys::window()
                .ok_or("Browser window unavailable")?
                .crypto()
                .map_err(|e| format!("Random source unavailable: {e:?}"))?;
            let mut bytes = [0_u8; 16];
            crypto
                .get_random_values_with_u8_array(&mut bytes)
                .map_err(|e| format!("Random identity failed: {e:?}"))?;
            let client_id = bytes.iter().map(|b| format!("{b:02x}")).collect();
            let client = Self(Rc::new(RefCell::new(Inner {
                worker: None,
                message: None,
                error: None,
                generation: 1,
                next_id: 1,
                client_id,
                pending: HashMap::new(),
                session: None,
            })));
            client.spawn_worker()?;
            Ok(client)
        }

        fn spawn_worker(&self) -> Result<(), String> {
            let options = WorkerOptions::new();
            options.set_type(WorkerType::Module);
            let worker = Worker::new_with_options("/nibli-learn/worker/bootstrap.js", &options)
                .map_err(|e| format!("Unable to start reasoning worker: {e:?}"))?;
            let weak = Rc::downgrade(&self.0);
            let message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                let Some(inner) = weak.upgrade() else { return };
                let json = if let Some(s) = event.data().as_string() {
                    Some(s)
                } else {
                    js_sys::JSON::stringify(&event.data())
                        .ok()
                        .and_then(|s| s.as_string())
                };
                let payload = json.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
                if let Some(notice) = payload.as_ref()
                    && notice.get("kind").and_then(|v| v.as_str()) == Some("phase")
                {
                    // Stage notifications do not commit a session, finish a request, or reset its watchdog.
                    return;
                }
                let Some(response) =
                    payload.and_then(|value| serde_json::from_value::<Response>(value).ok())
                else {
                    let mut state = inner.borrow_mut();
                    if let Some(worker) = state.worker.take() {
                        worker.terminate();
                    }
                    for (_, sender) in state.pending.drain() {
                        let _ = sender.send(Err(error("operational", "The worker returned an invalid response. Restore the last acknowledged session to continue.")));
                    }
                    return;
                };
                let mut state = inner.borrow_mut();
                if response.generation != state.generation.to_string() {
                    return;
                }
                let Some(sender) = state.pending.remove(&response.id) else {
                    return;
                };
                if let Ok(reply) = &response.result {
                    state.session = Some(reply.session.clone());
                }
                let _ = sender.send(response.result);
            });
            let weak = Rc::downgrade(&self.0);
            let on_error = Closure::<dyn FnMut(ErrorEvent)>::new(move |event: ErrorEvent| {
                let Some(inner) = weak.upgrade() else { return };
                let mut state = inner.borrow_mut();
                if let Some(worker) = state.worker.take() {
                    worker.terminate();
                }
                for (_, sender) in state.pending.drain() {
                    let _ = sender.send(Err(error("operational", format!("Reasoning worker failed: {}. Restore the last saved session to continue.", event.message()))));
                }
            });
            worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
            worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
            let mut state = self.0.borrow_mut();
            state.worker = Some(worker);
            state.message = Some(message);
            state.error = Some(on_error);
            Ok(())
        }

        pub fn last_session(&self) -> Option<SessionView> {
            self.0.borrow().session.clone()
        }

        pub async fn request(&self, command: Command) -> Result<Reply, WorkerError> {
            let (sender, receiver) = oneshot::channel();
            {
                let mut state = self.0.borrow_mut();
                let id = format!("{}-{}", state.client_id, state.next_id);
                state.next_id = state
                    .next_id
                    .checked_add(1)
                    .ok_or_else(|| error("operational", "Request identity exhausted."))?;
                let expected_revision = match command {
                    Command::Initialize { .. } | Command::Restore { .. } => None,
                    _ => state.session.as_ref().map(|s| s.revision.clone()),
                };
                let expected_epoch = match command {
                    Command::Initialize { .. } | Command::Restore { .. } => None,
                    _ => state.session.as_ref().map(|s| s.epoch.clone()),
                };
                let request = Request {
                    id: id.clone(),
                    generation: state.generation.to_string(),
                    expected_epoch,
                    expected_revision,
                    command,
                };
                let text = serde_json::to_string(&request)
                    .map_err(|e| error("serialization", e.to_string()))?;
                if text.len() > 16 * 1024 * 1024 {
                    return Err(error("validation", "Request exceeds 16 MiB."));
                }
                let Some(worker) = &state.worker else {
                    return Err(error("operational", "Reasoning worker is unavailable."));
                };
                worker
                    .post_message(&JsValue::from_str(&text))
                    .map_err(|e| error("operational", format!("Unable to send request: {e:?}")))?;
                state.pending.insert(id, sender);
            }
            // Main-thread watchdog remains responsive while synchronous WASM is busy.
            // No partial mutation is acknowledged or restored after termination.
            match select(receiver, Box::pin(TimeoutFuture::new(30_000))).await {
                Either::Left((result, _)) => {
                    result.map_err(|_| error("cancelled", "This worker request was cancelled."))?
                }
                Either::Right((_, _)) => {
                    let mut state = self.0.borrow_mut();
                    if let Some(worker) = state.worker.take() {
                        worker.terminate();
                    }
                    for (_, sender) in state.pending.drain() {
                        let _ = sender.send(Err(error("operational", "Worker time limit reached. No completed outcome was produced. Restore the last acknowledged session to continue.")));
                    }
                    Err(error(
                        "operational",
                        "Worker time limit reached. No completed outcome was produced. Restore the last acknowledged session to continue.",
                    ))
                }
            }
        }

        /// Hard cancellation of synchronous WASM, followed by recovery into a new worker.
        pub async fn cancel_and_restore(&self) -> Result<Reply, WorkerError> {
            let saved = {
                let mut state = self.0.borrow_mut();
                if let Some(worker) = state.worker.take() {
                    worker.set_onmessage(None);
                    worker.set_onerror(None);
                    worker.terminate();
                }
                for (_, sender) in state.pending.drain() {
                    let _ = sender.send(Err(error(
                        "cancelled",
                        "Run cancelled. No reasoning outcome was produced.",
                    )));
                }
                state.message = None;
                state.error = None;
                state.generation = state
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| error("operational", "Worker generation exhausted."))?;
                state.session.clone()
            };
            self.spawn_worker().map_err(|e| error("operational", e))?;
            match saved {
                Some(session) => self.request(Command::Restore { session }).await,
                None => Err(error(
                    "cancelled",
                    "Worker restarted. Open a lesson to initialize a session.",
                )),
            }
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use browser::WorkerClient;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use nibli_learn_model::{Command, Reply, SessionView, WorkerError};
    #[derive(Clone)]
    pub struct WorkerClient;
    impl WorkerClient {
        pub fn new() -> Result<Self, String> {
            Ok(Self)
        }
        pub fn last_session(&self) -> Option<SessionView> {
            None
        }
        pub async fn request(&self, _: Command) -> Result<Reply, WorkerError> {
            Err(WorkerError {
                kind: "unsupported".into(),
                message: "The interactive worker runs in the browser.".into(),
                record: None,
            })
        }
        pub async fn cancel_and_restore(&self) -> Result<Reply, WorkerError> {
            self.request(Command::Inspect).await
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use native::WorkerClient;
