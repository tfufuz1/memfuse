//! Client-side MCP dispatch mechanism for sending routed context to SLM endpoints.

use crate::router::RoutingDecision;
use memfuse_core::ipc::{JsonRpcRequest, JsonRpcResponse};
use memfuse_core::{MemFuseError, Result};
use serde_json::json;

/// Dispatches the prepared context from a [`RoutingDecision`] to the target SLM's MCP endpoint over HTTP JSON-RPC 2.0.
///
/// Sends ONLY the tailored [`memfuse_core::ContextWindow`] (not raw full search results) to `decision.profile.mcp_endpoint`.
pub async fn dispatch_to_slm(decision: &RoutingDecision) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| MemFuseError::Internal(format!("HTTP client build error: {e}")))?;

    let request_payload = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "slm_process_context".to_string(),
        params: json!({
            "context": decision.context,
            "profile_name": decision.profile.name,
        }),
    };

    let response = client
        .post(&decision.profile.mcp_endpoint)
        .json(&request_payload)
        .send()
        .await
        .map_err(|e| {
            MemFuseError::Internal(format!(
                "Fehler bei MCP-Dispatch an {}: {e}",
                decision.profile.mcp_endpoint
            ))
        })?;

    if !response.status().is_success() {
        return Err(MemFuseError::Internal(format!(
            "MCP-Endpunkt {} meldet HTTP Status {}",
            decision.profile.mcp_endpoint,
            response.status()
        )));
    }

    let rpc_response: JsonRpcResponse = response
        .json()
        .await
        .map_err(|e| MemFuseError::Internal(format!("Ungültige MCP JSON-RPC Antwort: {e}")))?;

    if let Some(error) = rpc_response.error {
        return Err(MemFuseError::Internal(format!(
            "MCP RPC Fehler [{}]: {}",
            error.code, error.message
        )));
    }

    if let Some(result) = rpc_response.result {
        if let Some(ans) = result.get("answer").and_then(|v| v.as_str()) {
            Ok(ans.to_string())
        } else {
            Ok(result.to_string())
        }
    } else {
        Err(MemFuseError::Internal(
            "MCP-Antwort enthielt weder result noch error".to_string(),
        ))
    }
}
