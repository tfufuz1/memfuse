// FILE-CONTEXT
// STAND:       2026-08-31T21:12:53Z (SESSION: 2c814094)
// ZWECK:       MCP JSON-RPC 2.0 Protokoll-Typen & DTO-Abbildung für MemFuse
// INVARIANTEN: DTO-Konvertierung aus MemFuseError muss saubere JSON-RPC 2.0 Codes und Error-Data tragen
// HOTSPOTS:    McpError::from(MemFuseError), JsonRpcResponse::from_error
// SIEHE AUCH:  ADR-010, memfuse-core/src/error_dto.rs

//! MCP JSON-RPC 2.0 Protokoll-Typen (Model Context Protocol Spec v2024-11-05)

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

pub use memfuse_core::ipc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// Helper function to convert an `McpError` directly into a `JsonRpcResponse`.
pub fn response_from_error(id: Option<Value>, err: McpError) -> JsonRpcResponse {
    JsonRpcResponse::err(id, err.code(), err.to_string())
}
