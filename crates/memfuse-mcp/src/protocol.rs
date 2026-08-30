//! MCP JSON-RPC 2.0 Protokoll-Typen (Model Context Protocol Spec v2024-11-05)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use memfuse_core::MemFuseErrorDto;

/// MCP Error representation matching standard JSON-RPC 2.0 error codes.
#[derive(Debug, Error)]
pub enum McpError {
    #[error("{message}")]
    ParseError {
        message: String,
        data: Option<Value>,
    },
    #[error("{message}")]
    InvalidRequest {
        message: String,
        data: Option<Value>,
    },
    #[error("{message}")]
    MethodNotFound {
        message: String,
        data: Option<Value>,
    },
    #[error("{message}")]
    InvalidParams {
        message: String,
        data: Option<Value>,
    },
    #[error("{message}")]
    InternalError {
        message: String,
        data: Option<Value>,
    },
}

impl McpError {
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self::ParseError {
            message: msg.into(),
            data: None,
        }
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::InvalidRequest {
            message: msg.into(),
            data: None,
        }
    }

    pub fn method_not_found(msg: impl Into<String>) -> Self {
        Self::MethodNotFound {
            message: msg.into(),
            data: None,
        }
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::InvalidParams {
            message: msg.into(),
            data: None,
        }
    }

    pub fn invalid_params_with_data(msg: impl Into<String>, data: Value) -> Self {
        Self::InvalidParams {
            message: msg.into(),
            data: Some(data),
        }
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::InternalError {
            message: msg.into(),
            data: None,
        }
    }

    pub fn internal_error_with_data(msg: impl Into<String>, data: Value) -> Self {
        Self::InternalError {
            message: msg.into(),
            data: Some(data),
        }
    }

    pub fn code(&self) -> i32 {
        match self {
            Self::ParseError { .. } => -32700,
            Self::InvalidRequest { .. } => -32600,
            Self::MethodNotFound { .. } => -32601,
            Self::InvalidParams { .. } => -32602,
            Self::InternalError { .. } => -32603,
        }
    }

    pub fn data(&self) -> Option<&Value> {
        match self {
            Self::ParseError { data, .. }
            | Self::InvalidRequest { data, .. }
            | Self::MethodNotFound { data, .. }
            | Self::InvalidParams { data, .. }
            | Self::InternalError { data, .. } => data.as_ref(),
        }
    }
}

impl From<memfuse_core::MemFuseError> for McpError {
    fn from(err: memfuse_core::MemFuseError) -> Self {
        let dto = MemFuseErrorDto::from(&err);
        let data_val = serde_json::to_value(&dto).ok();
        match err {
            memfuse_core::MemFuseError::InvalidInput(msg)
            | memfuse_core::MemFuseError::NotFound(msg) => match data_val {
                Some(d) => Self::invalid_params_with_data(msg, d),
                None => Self::invalid_params(msg),
            },
            other => {
                let msg = other.to_string();
                match data_val {
                    Some(d) => Self::internal_error_with_data(msg, d),
                    None => Self::internal_error(msg),
                }
            }
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

    /// Fehlerantwort mit data Payload gemäß JSON-RPC 2.0.
    pub fn err_with_data(
        id: Option<Value>,
        code: i32,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data,
            }),
        }
    }

    /// Convert an McpError directly into a JsonRpcResponse.
    pub fn from_error(id: Option<Value>, err: McpError) -> Self {
        Self::err_with_data(id, err.code(), err.to_string(), err.data().cloned())
    }
}
