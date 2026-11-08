//! Durable browser storage and verified offline installation boundaries.
//! Model/content validation belongs to the caller; this layer enforces schema/size/CAS.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkspaceEnvelope {
    pub schema: u32,
    pub revision: String,
    pub data: Value,
    pub updated_at: String,
    #[serde(default)]
    pub can_undo: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PlatformError {
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub current_revision: Option<String>,
}
impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for PlatformError {}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::*};
    use wasm_bindgen_futures::JsFuture;

    #[wasm_bindgen(module = "/js/platform.js")]
    extern "C" {
        #[wasm_bindgen(js_name = loadWorkspace)]
        fn load_js() -> js_sys::Promise;
        #[wasm_bindgen(js_name = saveWorkspace)]
        fn save_js(expected: JsValue, json: &str, backup: bool) -> js_sys::Promise;
        #[wasm_bindgen(js_name = undoWorkspace)]
        fn undo_js(expected: &str) -> js_sys::Promise;
        #[wasm_bindgen(js_name = rawExistingData)]
        fn raw_js() -> js_sys::Promise;
        #[wasm_bindgen(js_name = subscribeWorkspace)]
        fn subscribe_js(callback: &js_sys::Function) -> String;
        #[wasm_bindgen(js_name = unsubscribeWorkspace)]
        fn unsubscribe_js(token: &str);
        #[wasm_bindgen(js_name = offlineCatalog)]
        fn catalog_js() -> js_sys::Promise;
        #[wasm_bindgen(js_name = registerServiceWorker)]
        fn register_js() -> js_sys::Promise;
        #[wasm_bindgen(js_name = offlineStatus)]
        fn status_js() -> js_sys::Promise;
        #[wasm_bindgen(js_name = installOffline)]
        fn install_js(entry: &str, job: &str, callback: &js_sys::Function) -> js_sys::Promise;
        #[wasm_bindgen(js_name = cancelInstallation)]
        fn cancel_js(job: &str) -> bool;
        #[wasm_bindgen(js_name = installedContent)]
        fn content_js(id: &str) -> js_sys::Promise;
        #[wasm_bindgen(js_name = activateOffline)]
        fn activate_js(id: &str, expected: JsValue) -> js_sys::Promise;
        #[wasm_bindgen(js_name = undoOffline)]
        fn undo_offline_js(expected: &str) -> js_sys::Promise;
    }
    fn invalid(message: impl Into<String>) -> PlatformError {
        PlatformError {
            kind: "unavailable".into(),
            message: message.into(),
            current_revision: None,
        }
    }
    async fn decode<T: serde::de::DeserializeOwned>(
        promise: js_sys::Promise,
    ) -> Result<T, PlatformError> {
        let value = JsFuture::from(promise)
            .await
            .map_err(|error| invalid(format!("Browser platform failure: {error:?}")))?;
        let text = value
            .as_string()
            .ok_or_else(|| invalid("Browser platform returned an invalid response."))?;
        let envelope: Value =
            serde_json::from_str(&text).map_err(|error| invalid(error.to_string()))?;
        if envelope["ok"] == true {
            serde_json::from_value(envelope["value"].clone())
                .map_err(|error| invalid(error.to_string()))
        } else {
            Err(serde_json::from_value(envelope["error"].clone())
                .unwrap_or_else(|_| invalid("Browser platform operation failed.")))
        }
    }
    pub async fn load_workspace() -> Result<Option<WorkspaceEnvelope>, PlatformError> {
        decode(load_js()).await
    }
    pub async fn save_workspace(
        expected_revision: Option<String>,
        data: Value,
        backup: bool,
    ) -> Result<WorkspaceEnvelope, PlatformError> {
        let json = serde_json::to_string(&data).map_err(|error| invalid(error.to_string()))?;
        let expected = expected_revision
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL);
        decode(save_js(expected, &json, backup)).await
    }
    pub async fn undo_workspace(
        expected_revision: String,
    ) -> Result<WorkspaceEnvelope, PlatformError> {
        decode(undo_js(&expected_revision)).await
    }
    pub async fn raw_existing_data() -> Result<Value, PlatformError> {
        decode(raw_js()).await
    }

    pub struct WorkspaceSubscription {
        token: String,
        _callback: Closure<dyn FnMut(String)>,
    }
    impl Drop for WorkspaceSubscription {
        fn drop(&mut self) {
            unsubscribe_js(&self.token);
        }
    }
    pub fn subscribe_workspace_changes(
        mut callback: impl FnMut(Value) + 'static,
    ) -> WorkspaceSubscription {
        let callback = Closure::wrap(Box::new(move |text: String| {
            if let Ok(event) = serde_json::from_str(&text) {
                callback(event);
            }
        }) as Box<dyn FnMut(String)>);
        let token = subscribe_js(callback.as_ref().unchecked_ref());
        WorkspaceSubscription {
            token,
            _callback: callback,
        }
    }
    pub async fn offline_catalog() -> Result<Value, PlatformError> {
        decode(catalog_js()).await
    }
    pub async fn register_service_worker() -> Result<Value, PlatformError> {
        decode(register_js()).await
    }
    pub async fn offline_status() -> Result<Value, PlatformError> {
        decode(status_js()).await
    }
    pub async fn install_offline(
        entry_id: String,
        job_id: String,
        mut progress: impl FnMut(Value) + 'static,
    ) -> Result<Value, PlatformError> {
        let callback = Closure::wrap(Box::new(move |text: String| {
            if let Ok(value) = serde_json::from_str(&text) {
                progress(value);
            }
        }) as Box<dyn FnMut(String)>);
        let result = decode(install_js(
            &entry_id,
            &job_id,
            callback.as_ref().unchecked_ref(),
        ))
        .await;
        drop(callback);
        result
    }
    pub fn cancel_installation(job_id: &str) -> bool {
        cancel_js(job_id)
    }
    /// The caller must validate the returned course/reference models before activation.
    pub async fn installed_content(installation_id: String) -> Result<Value, PlatformError> {
        decode(content_js(&installation_id)).await
    }
    pub async fn activate_offline(
        installation_id: String,
        expected_revision: Option<String>,
    ) -> Result<Value, PlatformError> {
        decode(activate_js(
            &installation_id,
            expected_revision
                .map(JsValue::from)
                .unwrap_or(JsValue::NULL),
        ))
        .await
    }
    pub async fn undo_offline(expected_revision: String) -> Result<Value, PlatformError> {
        decode(undo_offline_js(&expected_revision)).await
    }
}
#[cfg(target_arch = "wasm32")]
pub use browser::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    fn unavailable() -> PlatformError {
        PlatformError {
            kind: "unsupported".into(),
            message: "Local browser storage is available in the web application.".into(),
            current_revision: None,
        }
    }
    pub async fn load_workspace() -> Result<Option<WorkspaceEnvelope>, PlatformError> {
        Err(unavailable())
    }
    pub async fn save_workspace(
        _: Option<String>,
        _: Value,
        _: bool,
    ) -> Result<WorkspaceEnvelope, PlatformError> {
        Err(unavailable())
    }
    pub async fn undo_workspace(_: String) -> Result<WorkspaceEnvelope, PlatformError> {
        Err(unavailable())
    }
    pub async fn raw_existing_data() -> Result<Value, PlatformError> {
        Err(unavailable())
    }
    pub struct WorkspaceSubscription;
    pub fn subscribe_workspace_changes(_: impl FnMut(Value) + 'static) -> WorkspaceSubscription {
        WorkspaceSubscription
    }
    pub async fn offline_catalog() -> Result<Value, PlatformError> {
        Err(unavailable())
    }
    pub async fn register_service_worker() -> Result<Value, PlatformError> {
        Err(unavailable())
    }
    pub async fn offline_status() -> Result<Value, PlatformError> {
        Err(unavailable())
    }
    pub async fn install_offline(
        _: String,
        _: String,
        _: impl FnMut(Value) + 'static,
    ) -> Result<Value, PlatformError> {
        Err(unavailable())
    }
    pub fn cancel_installation(_: &str) -> bool {
        false
    }
    pub async fn installed_content(_: String) -> Result<Value, PlatformError> {
        Err(unavailable())
    }
    pub async fn activate_offline(_: String, _: Option<String>) -> Result<Value, PlatformError> {
        Err(unavailable())
    }
    pub async fn undo_offline(_: String) -> Result<Value, PlatformError> {
        Err(unavailable())
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
