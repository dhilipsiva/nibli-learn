use dioxus::dioxus_core::spawn_forever;
use dioxus::prelude::*;
use nibli_learn_client::WorkerClient;
use nibli_learn_model::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "nibli-learn.local.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Course,
    Lesson,
    Sandbox,
    Reference,
    Data,
    Packs,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Knowledge,
    Replay,
    Proof,
}
#[derive(Clone, Debug)]
pub struct SavedRun {
    pub run: RunRecord,
    pub prediction: Option<String>,
    pub step: usize,
    pub bytes: usize,
    pub ordinal: usize,
}
impl SavedRun {
    pub fn new(run: RunRecord, prediction: Option<String>, step: usize) -> Self {
        struct Counter(usize);
        impl std::io::Write for Counter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.saturating_add(buf.len());
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut counter = Counter(0);
        let _ = serde_json::to_writer(&mut counter, &run);
        Self {
            run,
            prediction,
            step,
            bytes: counter.0,
            ordinal: 0,
        }
    }
}
#[derive(Clone, Copy)]
pub enum Purpose {
    Initialize,
    Change,
    Settings,
    Run,
    Cases,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AbortedAttempt {
    pub id: String,
    pub claim: String,
    pub epoch: String,
    pub revision: String,
    pub settings: RunSettings,
    pub status: String,
}

#[derive(Serialize, Deserialize)]
struct LocalState {
    schema: u32,
    progress: Progress,
    session: Option<SessionView>,
    theme: String,
    #[serde(default)]
    aborted: Vec<AbortedAttempt>,
    #[serde(default)]
    support_stage: usize,
}

pub struct AppState {
    pub learning: crate::curriculum::LearningState,
    pub client: Option<WorkerClient>,
    pub session: Option<SessionView>,
    pub progress: Progress,
    pub screen: Screen,
    pub pane: Pane,
    pub theme: String,
    pub notation: String,
    pub busy: bool,
    pub operation: String,
    pub serial: u64,
    pub notice: String,
    pub diagnostic: Option<WorkerError>,
    pub refusal: Option<String>,
    pub runs: Vec<SavedRun>,
    pub run_count: usize,
    pub aborted: Vec<AbortedAttempt>,
    pub active_attempt: Option<AbortedAttempt>,
    pub selected_run: Option<usize>,
    pub change_record: Option<Value>,
    pub show_change: bool,
    pub selected_event: usize,
    pub event_filter: String,
    pub event_search: String,
    pub playing: bool,
    pub playback_token: u64,
    pub proof_text: bool,
    pub proof_raw: bool,
    pub selected_node: Option<usize>,
    pub linked_assertion: Option<String>,
    pub drawer: Option<String>,
    pub reference: String,
    pub show_settings: bool,
    pub reset_dialog: bool,
    pub draft: String,
    pub source: String,
    pub note: String,
    pub quiz: Option<usize>,
    pub quiz_feedback: String,
    pub support_stage: usize,
    pub viewed_replay: bool,
    pub viewed_proof: bool,
    pub exercise_open: bool,
    pub policy: String,
    pub policy_draft: String,
    pub policy_revision: u32,
    pub checks_revision: Option<u32>,
    pub checks: Vec<CaseCheck>,
    pub explanation: String,
    pub exercise_feedback: String,
    pub complete: bool,
    pub storage_error: String,
    pub storage_locked: bool,
    pub import_text: String,
    pub import_error: String,
    pub export_message: String,
    pub sandbox_claim: String,
}

impl AppState {
    pub fn new() -> Self {
        let saved = load_local();
        let (progress, session, theme, storage_error, aborted, support_stage, storage_locked) =
            match saved {
                Ok(Some(s)) => (
                    s.progress,
                    s.session,
                    s.theme,
                    String::new(),
                    s.aborted,
                    s.support_stage,
                    false,
                ),
                Ok(None) => (
                    Progress::default(),
                    None,
                    "dark".into(),
                    String::new(),
                    vec![],
                    0,
                    false,
                ),
                Err(e) => (Progress::default(), None, "dark".into(), e, vec![], 0, true),
            };
        let learning = crate::curriculum::LearningState::new(&progress, session.is_some());
        let mut state = Self {
            learning,
            client: None,
            session,
            progress,
            screen: Screen::Course,
            pane: Pane::Knowledge,
            theme,
            notation: "both".into(),
            busy: true,
            operation: "loading".into(),
            serial: 0,
            notice: String::new(),
            diagnostic: None,
            refusal: None,
            runs: vec![],
            run_count: 0,
            selected_run: None,
            aborted,
            active_attempt: None,
            change_record: None,
            show_change: false,
            selected_event: 0,
            event_filter: "all".into(),
            event_search: String::new(),
            playing: false,
            playback_token: 0,
            proof_text: false,
            proof_raw: false,
            selected_node: None,
            linked_assertion: None,
            drawer: None,
            reference: "derivable".into(),
            show_settings: false,
            reset_dialog: false,
            draft: String::new(),
            source: String::new(),
            note: String::new(),
            quiz: None,
            quiz_feedback: String::new(),
            support_stage,
            viewed_replay: false,
            viewed_proof: false,
            exercise_open: false,
            policy: OR_POLICY.into(),
            policy_draft: OR_POLICY.into(),
            policy_revision: 1,
            checks_revision: None,
            checks: vec![],
            explanation: String::new(),
            exercise_feedback: String::new(),
            complete: false,
            storage_error,
            storage_locked,
            import_text: String::new(),
            import_error: String::new(),
            export_message: String::new(),
            sandbox_claim: CLAIM.into(),
        };
        let route = current_hash();
        if route.contains("/lesson/") {
            state.screen = Screen::Lesson;
        } else if route.contains("/sandbox") {
            state.screen = Screen::Sandbox;
        } else if route.contains("/reference") {
            state.screen = Screen::Reference;
        } else if route.contains("/data") {
            state.screen = Screen::Data;
        } else if route.contains("/packs") {
            state.screen = Screen::Packs;
        }
        if let Some(n) = route
            .split("/step/")
            .nth(1)
            .and_then(|s| s.split('?').next())
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| (1..=7).contains(n))
        {
            if n != state.progress.current_step {
                state.session = None;
                state.support_stage = 0;
            }
            state.progress.current_step = n;
        }
        if route.contains("tab=replay") {
            state.pane = Pane::Replay;
        }
        if route.contains("tab=proof") {
            state.pane = Pane::Proof;
        }
        if route.contains("run=") {
            state.notice="This link refers to a local run that is not available here. Open the lesson's authored setup or use the original device's exported run record.".into();
        }
        state
    }
    pub fn step(&self) -> usize {
        self.progress.current_step.clamp(1, 7)
    }
    pub fn prediction(&self) -> Option<String> {
        self.progress.steps[self.step() - 1].predicted.clone()
    }
    pub fn run(&self) -> Option<&RunRecord> {
        self.selected_run
            .and_then(|i| self.runs.get(i))
            .map(|r| &r.run)
    }
    pub fn saved_run(&self) -> Option<&SavedRun> {
        self.selected_run.and_then(|i| self.runs.get(i))
    }
    pub fn stale(&self) -> bool {
        self.run()
            .zip(self.session.as_ref())
            .is_some_and(|(r, s)| !r.is_current(s))
    }
    pub fn revision(&self) -> String {
        self.session
            .as_ref()
            .map(|s| s.revision.clone())
            .unwrap_or_else(|| "—".into())
    }
    pub fn settings(&self) -> RunSettings {
        self.session
            .as_ref()
            .map(|s| s.settings.clone())
            .unwrap_or_default()
    }
    pub fn active(&self, text: &str) -> usize {
        self.session
            .as_ref()
            .map(|s| {
                s.assertions
                    .iter()
                    .filter(|a| a.active && a.text == text)
                    .count()
            })
            .unwrap_or(0)
    }
    pub fn change_done(&self) -> bool {
        match self.step() {
            2 => self.active("approves(Reviewer, BuildB).") > 0,
            3 => self.active("approves(Reviewer, BuildA).") > 0,
            5 => match self.support_stage {
                0 => self.active("approves(Reviewer, BuildA).") >= 2,
                1 => self.active("approves(Reviewer, BuildA).") == 1,
                _ => self.active("approves(Reviewer, BuildA).") == 0,
            },
            _ => true,
        }
    }
    pub fn run_disabled(&self) -> bool {
        self.busy
            || self.client.is_none()
            || (self.screen != Screen::Sandbox
                && self.step() != 4
                && (self.prediction().is_none() || !self.change_done()))
    }
    pub fn execution_record(&self) -> Option<&Value> {
        if self.learning.failure.is_some() {
            return self.change_record.as_ref();
        }
        if !self.learning.is_legacy() || self.screen == Screen::Reference {
            self.learning.selected().map(|r| &r.record)
        } else {
            self.run().map(|r| &r.record)
        }
    }
    pub fn execution_epoch(&self) -> Option<&str> {
        if !self.learning.is_legacy() || self.screen == Screen::Reference {
            self.learning.selected().map(|r| r.epoch.as_str())
        } else {
            self.run().map(|r| r.epoch.as_str())
        }
    }
    pub fn events(&self) -> &[Value] {
        if self.show_change {
            return self
                .change_record
                .as_ref()
                .and_then(|v| v.get("events"))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
        }
        if !self.learning.is_legacy() || self.screen == Screen::Reference {
            return self
                .learning
                .selected()
                .and_then(|r| r.record.get("events"))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
        }
        self.run().map(|r| r.events()).unwrap_or(&[])
    }
    pub fn selected_event_value(&self) -> Option<Value> {
        self.events().get(self.selected_event).cloned()
    }
    pub fn retain_run(&mut self, mut run: SavedRun) {
        self.run_count += 1;
        run.ordinal = self.run_count;
        self.runs.push(run);
        let mut bytes: usize = self.runs.iter().map(|r| r.bytes).sum();
        let mut removed = 0;
        while self.runs.len() > 1 && (self.runs.len() > 8 || bytes > 64 * 1024 * 1024) {
            let oldest = self.runs.remove(0);
            bytes = bytes.saturating_sub(oldest.bytes);
            removed += 1;
        }
        self.selected_run = Some(self.runs.len() - 1);
        if removed > 0 {
            self.notice = format!(
                "Removed {removed} oldest run record(s) to keep history within 8 runs or 64 MiB. The newest run is retained. Export records you want to keep before running more queries."
            );
        }
    }
    pub fn abort_active_attempt(&mut self) {
        if let Some(mut attempt) = self.active_attempt.take() {
            attempt.status = "aborted".into();
            self.aborted.push(attempt);
            if self.aborted.len() > 24 {
                self.aborted.remove(0);
            }
        }
    }
}

pub fn boot(state: Signal<AppState>) {
    crate::local::boot(state);
}

pub fn send(mut state: Signal<AppState>, command: Command, purpose: Purpose) {
    let (client, serial, prediction, step, support_stage, policy_revision) = {
        let mut s = state.write();
        if s.busy {
            return;
        }
        let Some(client) = s.client.clone() else {
            return;
        };
        s.serial += 1;
        s.busy = true;
        s.operation = "running".into();
        s.playing = false;
        s.playback_token += 1;
        s.diagnostic = None;
        if matches!(purpose, Purpose::Run) {
            s.selected_run = None;
        }
        s.active_attempt = if let Command::Run { claim } = &command {
            s.session.as_ref().map(|session| AbortedAttempt {
                id: format!("attempt-{}", s.serial),
                claim: claim.clone(),
                epoch: session.epoch.clone(),
                revision: session.revision.clone(),
                settings: session.settings.clone(),
                status: "running".into(),
            })
        } else {
            None
        };
        (
            client,
            s.serial,
            s.prediction(),
            s.step(),
            s.support_stage,
            s.policy_revision,
        )
    };
    spawn_forever(async move {
        let result = client.request(command).await;
        if state.peek().serial != serial {
            return;
        }
        match result {
            Ok(reply) => {
                let mut s = state.write();
                s.session = Some(reply.session);
                s.busy = false;
                s.operation = "ready".into();
                s.active_attempt = None;
                s.refusal = None;
                s.diagnostic = None;
                if let Some(record) = reply.change_record {
                    s.change_record = Some(record);
                }
                if let Some(run) = reply.run {
                    let matched = prediction
                        .as_ref()
                        .is_some_and(|p| p == &run.outcome.status);
                    if matches!(purpose, Purpose::Run)
                        && matched
                        && s.screen != Screen::Sandbox
                        && step != 4
                        && step != 6
                        && step != 7
                    {
                        if step == 5 {
                            if support_stage >= 2 {
                                s.progress.steps[4].checked = true;
                            } else {
                                s.support_stage += 1;
                                s.progress.steps[4].predicted = None;
                            }
                        } else {
                            s.progress.steps[step - 1].checked = true;
                        }
                    }
                    s.retain_run(SavedRun::new(run, prediction, step));
                    s.selected_event = 0;
                    s.show_change = false;
                    s.operation = "complete".into();
                }
                if matches!(purpose, Purpose::Cases) {
                    s.checks = reply.checks;
                    s.checks_revision = Some(policy_revision);
                    s.exercise_feedback = if s.checks.iter().all(|c| c.passed) {
                        "All four cases match the intended behavior. Review your explanation, then check the repair.".into()
                    } else {
                        "Some cases differ from the intended behavior. Inspect the cases that reveal the policy's bug.".into()
                    };
                    s.operation = "complete".into();
                }
                if matches!(purpose, Purpose::Change) {
                    s.draft.clear();
                    s.source.clear();
                }
                drop(s);
                persist(state);
            }
            Err(error) => {
                let mut s = state.write();
                s.busy = false;
                if matches!(
                    error.kind.as_str(),
                    "operational" | "serialization" | "cancelled"
                ) {
                    s.abort_active_attempt();
                } else {
                    s.active_attempt = None;
                }
                if let Some(record) = error.record.clone() {
                    s.change_record = Some(record);
                }
                if error.kind == "cancelled" {
                    s.operation = "cancelled".into();
                } else if error.kind == "admission" || error.kind == "refusal" {
                    s.operation = "ready".into();
                    s.refusal = Some(error.message.clone());
                    s.diagnostic = Some(error);
                } else {
                    s.operation = if matches!(
                        error.kind.as_str(),
                        "operational" | "serialization" | "unsupported"
                    ) {
                        "error".into()
                    } else {
                        "ready".into()
                    };
                    s.diagnostic = Some(error);
                }
                drop(s);
                persist(state);
            }
        }
    });
}

pub fn run_claim(state: Signal<AppState>) {
    let claim = if state.peek().screen == Screen::Sandbox {
        state.peek().sandbox_claim.clone()
    } else {
        CLAIM.into()
    };
    send(state, Command::Run { claim }, Purpose::Run);
}

pub fn cancel(mut state: Signal<AppState>) {
    let Some(client) = state.peek().client.clone() else {
        return;
    };
    {
        let mut s = state.write();
        s.abort_active_attempt();
        s.serial += 1;
        s.busy = true;
        s.operation = "recovering".into();
        s.selected_run = None;
        s.playing = false;
        s.playback_token += 1;
    }
    spawn_forever(async move {
        match client.cancel_and_restore().await {
            Ok(reply) => {
                let mut s = state.write();
                s.session = Some(reply.session);
                s.busy = false;
                s.operation = "cancelled".into();
                s.diagnostic = None;
                s.notice = "Cancelled. The worker was replaced and the last committed knowledge restored. No completed outcome was produced.".into();
                drop(s);
                persist(state);
            }
            Err(e) => {
                let mut s = state.write();
                s.busy = false;
                s.operation = "error".into();
                s.diagnostic = Some(e);
            }
        }
    });
}

pub fn open_step(mut state: Signal<AppState>, step: usize, force: bool) {
    if state.peek().busy || !(1..=7).contains(&step) {
        return;
    }
    crate::local::capture(state);
    let release_project = {
        let s = state.peek();
        if s.learning.is_legacy() {
            s.learning.workspace.active_project.clone().filter(|id| {
                s.learning
                    .workspace
                    .projects
                    .get(id)
                    .is_some_and(|p| p.lesson.as_deref() == Some(LESSON_ID))
            })
        } else {
            None
        }
    }
    .unwrap_or_else(|| "original-release".into());
    let preserve = {
        let s = state.peek();
        !force
            && step == s.step() + 1
            && match step {
                2 => s.active("approves(TestRunner, BuildA).") > 0,
                3 => s.active("approves(Reviewer, BuildB).") > 0,
                4 | 5 => s.active("approves(Reviewer, BuildA).") > 0,
                6 => s.active("approves(Reviewer, BuildA).") == 0,
                7 => true,
                _ => false,
            }
    };
    {
        let mut s = state.write();
        s.learning.workspace.progress.current = LearningSelection {
            lesson: LESSON_ID.into(),
            activity: RELEASE_OBJECTIVES[step - 1].into(),
        };
        s.learning.workspace.active_project = Some(release_project);
        s.progress.current_step = step;
        s.screen = Screen::Lesson;
        s.complete = false;
        s.pane = Pane::Knowledge;
        s.refusal = None;
        s.diagnostic = None;
        s.quiz = None;
        s.quiz_feedback.clear();
        s.draft.clear();
        s.source.clear();
        s.note.clear();
        s.support_stage = 0;
        s.exercise_open = false;
        s.viewed_replay = false;
        s.viewed_proof = false;
        s.playing = false;
        s.playback_token += 1;
        if preserve {
            s.notice.clear();
        } else {
            s.selected_run = None;
            s.notice = "This step opened from its authored setup. Earlier run records remain attached to their original knowledge.".into();
        }
    }
    update_route(state);
    if !preserve {
        send(
            state,
            Command::Initialize {
                epoch: epoch(),
                statements: lesson_setup(step),
                settings: RunSettings::default(),
            },
            Purpose::Initialize,
        );
    } else {
        persist(state);
    }
}

pub fn select_prediction(mut state: Signal<AppState>, prediction: &str) {
    let mut s = state.write();
    let step = s.step();
    s.progress.steps[step - 1].predicted = Some(prediction.into());
    drop(s);
    persist(state);
}
pub fn open_pane(mut state: Signal<AppState>, pane: Pane) {
    let mut s = state.write();
    s.playing = false;
    s.playback_token += 1;
    s.pane = pane;
    if pane == Pane::Replay && s.run().is_some() {
        s.viewed_replay = true;
    }
    if pane == Pane::Proof && s.run().is_some() {
        s.viewed_proof = true;
    }
    drop(s);
    update_route(state);
}
pub fn navigate(mut state: Signal<AppState>, screen: Screen) {
    state.write().screen = screen;
    update_route(state);
}
pub fn update_route(state: Signal<AppState>) {
    let s = state.peek();
    let route = match s.screen {
        Screen::Course => "#/course".into(),
        Screen::Lesson if !s.learning.is_legacy() => format!(
            "#/lesson/{}/activity/{}",
            s.learning.workspace.progress.current.lesson,
            s.learning.workspace.progress.current.activity
        ),
        Screen::Lesson => format!(
            "#/lesson/{LESSON_ID}/step/{}?tab={}",
            s.step(),
            match s.pane {
                Pane::Knowledge => "knowledge",
                Pane::Replay => "replay",
                Pane::Proof => "proof",
            }
        ),
        Screen::Sandbox => "#/sandbox/local".into(),
        Screen::Reference => "#/reference".into(),
        Screen::Data => "#/data".into(),
        Screen::Packs => "#/packs".into(),
    };
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_hash(&route);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = route;
}

pub fn persist(state: Signal<AppState>) {
    crate::local::persist(state, false);
}

fn load_local() -> Result<Option<LocalState>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window()
            .ok_or("Browser storage is unavailable.")?
            .local_storage()
            .map_err(|_| "Browser storage is unavailable.")?
            .ok_or("Browser storage is unavailable.")?;
        let Some(text) = storage
            .get_item(STORAGE_KEY)
            .map_err(|_| "Unable to read local progress.")?
        else {
            return Ok(None);
        };
        let state: LocalState = serde_json::from_str(&text).map_err(|_| "Saved state could not be read. It has been retained; export it before resetting browser storage.")?;
        if state.schema != 1 || !matches!(state.theme.as_str(), "dark" | "light") {
            return Err(
                "Saved data uses an incompatible version. Existing browser data was retained."
                    .into(),
            );
        }
        state.progress.validate()?;
        Ok(Some(state))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(None)
    }
}

pub fn current_hash() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        String::new()
    }
}
pub fn epoch() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        format!(
            "lesson-{}-{}",
            js_sys::Date::now() as u64,
            (js_sys::Math::random() * 1_000_000_000.0) as u64
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "native-preview".into()
    }
}

pub fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default()
}
pub fn event_failed(event: &Value) -> bool {
    let result = text(event, "result").to_lowercase();
    result.contains("false") || result.contains("failed") || text(event, "kind").contains("fail")
}
pub fn event_title(event: &Value) -> String {
    let detail = text(event, "detail");
    if detail.is_empty() {
        text(event, "kind").replace('_', " ")
    } else {
        detail
    }
}
pub fn event_proof_node(event: &Value) -> Option<usize> {
    event.get("proof_node").and_then(|v| {
        v.as_u64()
            .map(|n| n as usize)
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

pub fn download(filename: &str, json: String) {
    let filename =
        serde_json::to_string(filename).unwrap_or_else(|_| "\"nibli-learn.json\"".into());
    let content = serde_json::to_string(&json).unwrap_or_else(|_| "\"\"".into());
    spawn(async move {
        let _ = document::eval(&format!("const blob = new Blob([{content}], {{type:'application/json'}}); const url = URL.createObjectURL(blob); const a = document.createElement('a'); a.href=url; a.download={filename}; a.click(); setTimeout(()=>URL.revokeObjectURL(url),1000); return true;")).await;
    });
}

pub fn focus(id: &str) {
    let id = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".into());
    spawn_forever(async move {
        gloo_timers::future::TimeoutFuture::new(0).await;
        let _ = document::eval(&format!(
            "document.getElementById({id})?.focus(); return true;"
        ))
        .await;
    });
}

#[cfg(target_arch = "wasm32")]
pub struct Navigation(wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>);
#[cfg(target_arch = "wasm32")]
impl Drop for Navigation {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        if let Some(window) = web_sys::window() {
            let _ = window
                .remove_event_listener_with_callback("hashchange", self.0.as_ref().unchecked_ref());
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub fn install_navigation(mut state: Signal<AppState>) -> Navigation {
    use wasm_bindgen::JsCast;
    let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: web_sys::Event| {
        let hash = current_hash();
        if hash == "#main-content" {
            focus("main-content");
            return;
        }
        let screen = if hash.starts_with("#/lesson/") {
            Screen::Lesson
        } else if hash.starts_with("#/sandbox") {
            Screen::Sandbox
        } else if hash.starts_with("#/reference") {
            Screen::Reference
        } else if hash.starts_with("#/data") {
            Screen::Data
        } else if hash.starts_with("#/packs") {
            Screen::Packs
        } else {
            Screen::Course
        };
        if let Some((lesson, activity)) = crate::curriculum::selection_from_hash(&hash) {
            if state.peek().learning.workspace.progress.current.activity != activity
                || state.peek().learning.workspace.progress.current.lesson != lesson
            {
                crate::curriculum::open_activity(state, &lesson, &activity, false);
            }
            return;
        }
        let step = hash
            .split("/step/")
            .nth(1)
            .and_then(|v| v.split('?').next())
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| (1..=7).contains(n));
        if let Some(step) = step
            && (step != state.peek().step() || !state.peek().learning.is_legacy())
        {
            if state.peek().busy {
                update_route(state);
                return;
            }
            open_step(state, step, true);
        }
        let mut s = state.write();
        s.screen = screen;
        if screen == Screen::Lesson {
            s.pane = if hash.contains("tab=replay") {
                Pane::Replay
            } else if hash.contains("tab=proof") {
                Pane::Proof
            } else {
                Pane::Knowledge
            };
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    if let Some(window) = web_sys::window() {
        let _ = window
            .add_event_listener_with_callback("hashchange", callback.as_ref().unchecked_ref());
    }
    Navigation(callback)
}
