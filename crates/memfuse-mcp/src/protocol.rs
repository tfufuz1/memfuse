//! MCP JSON-RPC 2.0 Protokoll-Typen (Model Context Protocol Spec v2024-11-05)

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Eingehende JSON-RPC 2.0 Nachricht (Request oder Notification).
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Fehlerobjekt.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
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
