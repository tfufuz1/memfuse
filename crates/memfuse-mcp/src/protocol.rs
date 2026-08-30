//! MCP JSON-RPC 2.0 Protokoll-Typen (Model Context Protocol Spec v2024-11-05)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// MCP Error representation matching standard JSON-RPC 2.0 error codes.
#[derive(Debug, Error)]
pub enum McpError {
    #[error("{0}")]
    ParseError(String),
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0}")]
    MethodNotFound(String),
    #[error("{0}")]
    InvalidParams(String),
    #[error("{0}")]
    InternalError(String),
}

impl McpError {
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self::ParseError(msg.into())
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::InvalidRequest(msg.into())
    }

    pub fn method_not_found(msg: impl Into<String>) -> Self {
        Self::MethodNotFound(msg.into())
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::InvalidParams(msg.into())
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::InternalError(msg.into())
    }

    pub fn code(&self) -> i32 {
        match self {
            Self::ParseError(_) => -32700,
            Self::InvalidRequest(_) => -32600,
            Self::MethodNotFound(_) => -32601,
            Self::InvalidParams(_) => -32602,
            Self::InternalError(_) => -32603,
        }
    }
}

impl From<memfuse_core::MemFuseError> for McpError {
    fn from(err: memfuse_core::MemFuseError) -> Self {
        match err {
            memfuse_core::MemFuseError::InvalidInput(msg)
            | memfuse_core::MemFuseError::NotFound(msg) => Self::invalid_params(msg),
            other => Self::internal_error(other.to_string()),
        }
    }
}

impl From<String> for McpError {
    fn from(msg: String) -> Self {
        Self::invalid_params(msg)
    }
}

impl From<&str> for McpError {
    fn from(msg: &str) -> Self {
        Self::invalid_params(msg.to_string())
    }
}

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

    /// Convert an McpError directly into a JsonRpcResponse.
    pub fn from_error(id: Option<Value>, err: McpError) -> Self {
        Self::err(id, err.code(), err.to_string())
    }
}
