//! Client-side MCP dispatch mechanism for sending routed context to SLM endpoints.
// STAND: 2026-09-02T17:10:00Z (SESSION: 20260902)
// ZWECK: Stdio JSON-RPC 2.0 Dispatcher für SLM-Endpunkte gemäß ADR-010.

use crate::router::RoutingDecision;
use memfuse_core::ipc::{JsonRpcRequest, JsonRpcResponse};
use memfuse_core::{MemFuseError, Result};
use serde_json::json;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// Dispatches the prepared context from a [`RoutingDecision`] to the target SLM's MCP endpoint
/// over stdio JSON-RPC 2.0 (ADR-010 compliant).
///
/// Sends ONLY the tailored [`memfuse_core::ContextWindow`] (not raw full search results)
/// to the executable or script specified in `decision.profile.mcp_endpoint`.
pub async fn dispatch_to_slm(decision: &RoutingDecision) -> Result<String> {
    let endpoint = decision.profile.mcp_endpoint.trim();
    if endpoint.is_empty() {
        return Err(MemFuseError::InvalidInput("Empty MCP endpoint".into()));
    }

    let request_payload = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "slm_process_context".to_string(),
        params: json!({
            "context": decision.context,
            "profile_name": decision.profile.name,
        }),
    };

    let mut payload_bytes = serde_json::to_vec(&request_payload)?;
    payload_bytes.push(b'\n');

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(endpoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            MemFuseError::Internal(format!(
                "Fehler bei MCP-Dispatch an {}: {e}",
                decision.profile.mcp_endpoint
            ))
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&payload_bytes)
            .await
            .map_err(MemFuseError::Io)?;
        stdin.flush().await.map_err(MemFuseError::Io)?;
    } else {
        return Err(MemFuseError::Internal(format!(
            "Fehler bei MCP-Dispatch an {}: Failed to open child stdin",
            decision.profile.mcp_endpoint
        )));
    }

    let stdout = child.stdout.take().ok_or_else(|| {
        MemFuseError::Internal(format!(
            "Fehler bei MCP-Dispatch an {}: Failed to open child stdout",
            decision.profile.mcp_endpoint
        ))
    })?;

    let timeout_duration = std::time::Duration::from_secs(30);
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    let read_result = tokio::time::timeout(timeout_duration, reader.read_line(&mut line)).await;
    match read_result {
        Ok(Ok(0)) | Ok(Err(_)) => {
            return Err(MemFuseError::Internal(format!(
                "Fehler bei MCP-Dispatch an {}: Process closed stdout without sending JSON-RPC response",
                decision.profile.mcp_endpoint
            )));
        }
        Err(_) => {
            let _ = child.kill().await;
            return Err(MemFuseError::Internal(format!(
                "Fehler bei MCP-Dispatch an {}: Timeout",
                decision.profile.mcp_endpoint
            )));
        }
        Ok(Ok(_)) => {}
    }

    let _ = child.wait().await;

    let rpc_response: JsonRpcResponse = serde_json::from_str(&line)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::SlmProfile;
    use memfuse_core::{ContextChunk, ContextWindow, DocId, TokenBudget};

    #[tokio::test]
    async fn test_dispatch_to_slm_invalid_endpoint_fails_gracefully() {
        let profile = SlmProfile::new(
            "test-slm",
            "/nonexistent/binary/path/12345",
            vec![],
            TokenBudget::default(),
            0.5,
        );
        let decision = RoutingDecision {
            profile,
            context: ContextWindow {
                chunks: vec![ContextChunk {
                    doc_id: DocId::new(1),
                    content: "hello world".into(),
                    relevance: 0.9,
                    token_count: 2,
                    metadata: None,
                    contextual_prefix: None,
                    links: vec![],
                }],
                total_tokens: 2,
                truncated: false,
            },
            confidence: None,
        };

        let result = dispatch_to_slm(&decision).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Fehler bei MCP-Dispatch"));
    }
}
