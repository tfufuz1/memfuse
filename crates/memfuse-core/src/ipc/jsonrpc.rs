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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_rpc_response_ok() {
        let resp = JsonRpcResponse::ok(Some(json!(42)), json!({"status": "success"}));
        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, Some(json!(42)));
        assert!(resp.error.is_none());
        assert_eq!(resp.result, Some(json!({"status": "success"})));

        let ser = serde_json::to_string(&resp).expect("serialize response ok"); // expect
        let deser: JsonRpcResponse = serde_json::from_str(&ser).expect("deserialize response ok"); // expect
        assert_eq!(deser.jsonrpc, "2.0");
        assert_eq!(deser.id, Some(json!(42)));
        assert_eq!(deser.result, Some(json!({"status": "success"})));
        assert!(deser.error.is_none());
    }

    #[test]
    fn test_json_rpc_response_err() {
        let resp = JsonRpcResponse::err(Some(json!("req-1")), -32600, "Invalid Request");
        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, Some(json!("req-1")));
        assert!(resp.result.is_none());

        let err = resp.error.as_ref().expect("error payload present"); // expect
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Invalid Request");
        assert!(err.data.is_none());

        let ser = serde_json::to_string(&resp).expect("serialize response err"); // expect
        let deser: JsonRpcResponse = serde_json::from_str(&ser).expect("deserialize response err"); // expect
        assert_eq!(deser.error.expect("error present").code, -32600); // expect
    }

    #[test]
    fn test_json_rpc_request_deserialization_and_notification() {
        let req_json = r#"{"jsonrpc": "2.0", "id": 1, "method": "ping", "params": {"key": "val"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(req_json).expect("parse request"); // expect
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.id, Some(json!(1)));
        assert_eq!(req.method, "ping");
        assert_eq!(req.params["key"], "val");

        // Notification (no id)
        let notif_json = r#"{"jsonrpc": "2.0", "method": "notify"}"#;
        let notif: JsonRpcRequest = serde_json::from_str(notif_json).expect("parse notification"); // expect
        assert_eq!(notif.jsonrpc, "2.0");
        assert!(notif.id.is_none());
        assert_eq!(notif.method, "notify");
        assert_eq!(notif.params, json!(null));
    }

    #[test]
    fn test_json_rpc_error_data_payload() {
        let err = JsonRpcError {
            code: -32000,
            message: "Server error".to_string(),
            data: Some(json!({"details": "database locked"})),
        };
        let ser = serde_json::to_string(&err).expect("serialize error"); // expect
        let deser: JsonRpcError = serde_json::from_str(&ser).expect("deserialize error"); // expect
        assert_eq!(deser.code, -32000);
        assert_eq!(
            deser.data.expect("data present")["details"], // expect
            "database locked"
        );
    }
}
