//! One explicit request at a time; the secret is deliberately absent from DTOs.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelConfig {
    pub protocol: String,
    pub endpoint: String,
    pub model: String,
    pub timeout_ms: u32,
    pub max_response_bytes: u32,
}
impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            protocol: "ollama".into(),
            endpoint: "http://127.0.0.1:11434/api/chat".into(),
            model: String::new(),
            timeout_ms: 60000,
            max_response_bytes: 65536,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelMessage {
    pub role: String,
    pub content: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPreview {
    pub protocol: String,
    pub endpoint: String,
    pub body: String,
    pub timeout_ms: u32,
    pub max_response_bytes: u32,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ModelReply {
    pub text: String,
    pub truncated: bool,
    pub bytes: usize,
    pub model: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ModelError {
    pub kind: String,
    pub message: String,
}
#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    preview: Option<T>,
    reply: Option<T>,
    error: Option<ModelError>,
}
fn unpack<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, ModelError> {
    let value: Envelope<T> = serde_json::from_str(text).map_err(|_| ModelError {
        kind: "malformed".into(),
        message: "Invalid transport envelope.".into(),
    })?;
    if value.ok {
        value.preview.or(value.reply).ok_or(ModelError {
            kind: "malformed".into(),
            message: "Missing transport result.".into(),
        })
    } else {
        Err(value.error.unwrap_or(ModelError {
            kind: "unavailable".into(),
            message: "Model operation unavailable.".into(),
        }))
    }
}
#[cfg(target_arch = "wasm32")]
mod browser {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen(module = "/js/tutor.js")]
    extern "C" {
        #[wasm_bindgen(js_name=prepareModel)]
        pub fn prepare(config: &str, messages: &str) -> String;
        #[wasm_bindgen(js_name=sendModel)]
        pub async fn send(preview: &str, secret: &str, id: &str) -> JsValue;
        #[wasm_bindgen(js_name=cancelModel)]
        pub fn cancel(id: &str) -> bool;
    }
}
pub fn prepare(
    config: &ModelConfig,
    messages: &[ModelMessage],
) -> Result<ModelPreview, ModelError> {
    #[cfg(target_arch = "wasm32")]
    {
        unpack(&browser::prepare(
            &serde_json::to_string(config).unwrap(),
            &serde_json::to_string(messages).unwrap(),
        ))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (config, messages);
        unpack(
            r#"{"ok":false,"error":{"kind":"unsupported","message":"Model transport runs only in the browser."}}"#,
        )
    }
}
pub async fn send(
    preview: &ModelPreview,
    secret: &str,
    id: &str,
) -> Result<ModelReply, ModelError> {
    #[cfg(target_arch = "wasm32")]
    {
        let value = browser::send(&serde_json::to_string(preview).unwrap(), secret, id).await;
        unpack(&value.as_string().unwrap_or_default())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (preview, secret, id);
        unpack(
            r#"{"ok":false,"error":{"kind":"unsupported","message":"Model transport runs only in the browser."}}"#,
        )
    }
}
pub fn cancel(id: &str) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        browser::cancel(id)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
        false
    }
}
