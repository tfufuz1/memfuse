//! Generic JSON-RPC 2.0 protocol types for `MemFuse` IPC.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Eingehende JSON-RPC 2.0 Nachricht (Request oder Notification).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Immer "2.0".
    pub jsonrpc: String,
    /// `None` bei Notifications (keine Antwort erwartet).
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Ausgehende JSON-RPC 2.0 Nachricht.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Immer "2.0".
    pub jsonrpc: String,
    /// Request ID (`None` bei Parse/Batch Errors).
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Fehlerobjekt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Numerischer Fehlercode gemäß JSON-RPC 2.0 Spezifikation.
    pub code: i32,
    /// Kurze Fehlerbeschreibung.
    pub message: String,
    /// Ggfs. zusätzliche Details/Kontextdaten.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    /// Erfolgreiche Antwort.
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Fehlerantwort gemäß JSON-RPC 2.0.
    pub fn err(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}
