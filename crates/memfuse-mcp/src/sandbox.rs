// memfuse-mcp/src/sandbox.rs
// MCP Tool Isolation Layer (Anthropic Containment Pattern)

use memfuse_core::{MemFuseError, Result};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;

/// Erlaubte MCP-Tool-Kategorien (Whitelist-Prinzip).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCategory {
    /// Read-only Datenbankoperationen (immer erlaubt).
    DatabaseRead,
    /// Schreibende Datenbankoperationen (erfordern explizite Freigabe).
    DatabaseWrite,
    /// Externe Code-Ausführung (höchste Risikostufe, standardmäßig gesperrt).
    CodeExecution,
}

/// Konfiguration der Sandbox-Policy.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    pub allow_db_reads: bool,
    pub allow_db_writes: bool,
    pub allow_code_execution: bool,
    /// Maximale Ausführungszeit pro Tool-Call in Millisekunden.
    pub max_execution_ms: u64,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            allow_db_reads: true,
            allow_db_writes: false,      // Schreibzugriff explizit opt-in
            allow_code_execution: false, // Code-Ausführung standardmäßig gesperrt
            max_execution_ms: 5_000,
        }
    }
}

/// Volatiler Tool-Output-Speicher mit Zeroize-Garantie bei Drop.
///
/// Tool-Ergebnisse werden verschlüsselt im Arbeitsspeicher gehalten.
/// Beim Drop wird der Speicher via Zeroize bereinigt.
pub struct VolatileToolResult {
    /// Verschlüsselte Ausgabe (AES-256-GCM-SIV via memfuse-crypto).
    encrypted: zeroize::Zeroizing<Vec<u8>>,
    /// Klartext-Nonce (nicht sensitiv).
    nonce: Vec<u8>,
}

impl VolatileToolResult {
    /// Speichert einen Tool-Output verschlüsselt.
    pub fn encrypt(plaintext: &[u8], key: &memfuse_crypto::CryptoKey) -> Result<Self> {
        let (encrypted, nonce) = key
            .encrypt_auto_nonce(plaintext)
            .map_err(|e| MemFuseError::Internal(format!("Sandbox encrypt: {e}")))?;
        Ok(Self {
            encrypted: zeroize::Zeroizing::new(encrypted),
            nonce: nonce.to_vec(),
        })
    }

    /// Entschlüsselt und gibt den Klartext zurück.
    pub fn decrypt(&self, key: &memfuse_crypto::CryptoKey) -> Result<Vec<u8>> {
        if self.nonce.len() != 12 {
            return Err(MemFuseError::Internal(
                "Sandbox decrypt: Invalid nonce length".into(),
            ));
        }
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&self.nonce);
        key.decrypt_auto_nonce(&self.encrypted, &nonce_bytes)
            .map_err(|e| MemFuseError::Internal(format!("Sandbox decrypt: {e}")))
    }
}

/// MCP Sandbox: validiert Tool-Calls und verwaltet volatile Ergebnisse.
pub struct McpSandbox {
    policy: SandboxPolicy,
    /// Session-lokale volatile Ergebnisse (werden bei Session-Ende gedropt).
    volatile_results: Mutex<HashMap<String, VolatileToolResult>>,
    /// Session-Schlüssel (wird bei Drop zeroized).
    session_key: memfuse_crypto::CryptoKey,
}

impl McpSandbox {
    /// Erstellt eine neue Sandbox-Instanz mit frischem Sitzungsschlüssel.
    pub fn new(policy: SandboxPolicy) -> Self {
        use rand::RngCore;
        let mut salt = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt);
        let mut passphrase = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut passphrase);
        let key = memfuse_crypto::CryptoKey::try_new(&hex::encode(passphrase), &salt)
            .expect("CryptoKey initialization failed in McpSandbox"); // expect

        Self {
            policy,
            volatile_results: Mutex::new(HashMap::new()),
            session_key: key,
        }
    }

    /// Gibt die aktuell konfigurierte SandboxPolicy zurück.
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Validiert ob ein Tool-Call erlaubt ist.
    pub fn validate_tool_call(&self, method: &str, _params: &Value) -> Result<()> {
        let category = Self::classify_method(method);
        match category {
            ToolCategory::DatabaseRead => {
                if !self.policy.allow_db_reads {
                    return Err(MemFuseError::InvalidInput(format!(
                        "Sandbox: DB-Lesezugriff gesperrt für '{method}'"
                    )));
                }
            }
            ToolCategory::DatabaseWrite => {
                if !self.policy.allow_db_writes {
                    return Err(MemFuseError::InvalidInput(format!(
                        "Sandbox: DB-Schreibzugriff gesperrt für '{method}'"
                    )));
                }
            }
            ToolCategory::CodeExecution => {
                if !self.policy.allow_code_execution {
                    return Err(MemFuseError::InvalidInput(
                        "Sandbox: Code-Ausführung ist gesperrt (SandboxPolicy)".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Klassifiziert die MCP-Methode bzw. den Tool-Namen in eine `ToolCategory`.
    pub fn classify_method(method: &str) -> ToolCategory {
        match method {
            "memfuse_search" | "memfuse_get" | "memfuse_collections" => ToolCategory::DatabaseRead,
            "memfuse_insert" | "memfuse_delete" | "memfuse_upsert" => ToolCategory::DatabaseWrite,
            _ => ToolCategory::CodeExecution,
        }
    }

    /// Führt eine Asynchrone Tool-Future mit Timeout gemäß SandboxPolicy aus.
    pub async fn execute_with_timeout<F, T, E>(
        &self,
        tool_name: &str,
        fut: F,
    ) -> std::result::Result<T, E>
    where
        F: std::future::Future<Output = std::result::Result<T, E>>,
        E: From<MemFuseError>,
    {
        let duration = std::time::Duration::from_millis(self.policy.max_execution_ms);
        tokio::time::timeout(duration, fut).await.map_err(|_| {
            E::from(MemFuseError::Internal(format!(
                "Tool '{}' überschritt Timeout {}ms",
                tool_name, self.policy.max_execution_ms
            )))
        })?
    }

    /// Speichert einen Tool-Output verschlüsselt in der Session.
    pub fn store_volatile(&self, key: &str, output: &[u8]) -> Result<()> {
        let volatile_res = VolatileToolResult::encrypt(output, &self.session_key)?;
        let mut results = self.volatile_results.lock();
        results.insert(key.to_string(), volatile_res);
        Ok(())
    }

    /// Ruft einen verschlüsselten Tool-Output ab und entschlüsselt ihn.
    pub fn get_volatile(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let results = self.volatile_results.lock();
        if let Some(res) = results.get(key) {
            let plaintext = res.decrypt(&self.session_key)?;
            Ok(Some(plaintext))
        } else {
            Ok(None)
        }
    }
}

impl Drop for McpSandbox {
    fn drop(&mut self) {
        // volatile_results wird gedroppt → Zeroizing<Vec<u8>> nullt den Speicher
        self.session_key.emergency_wipe();
        tracing::debug!("McpSandbox dropped: volatile tool results zeroized");
    }
}

// simple hex encoder helper to avoid external crate dependency overhead if needed, or use std formatting
mod hex {
    pub fn encode(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_default_policy() {
        let sandbox = McpSandbox::new(SandboxPolicy::default());

        assert!(sandbox
            .validate_tool_call("memfuse_search", &Value::Null)
            .is_ok());
        assert!(sandbox
            .validate_tool_call("memfuse_get", &Value::Null)
            .is_ok());
        assert!(sandbox
            .validate_tool_call("memfuse_collections", &Value::Null)
            .is_ok());

        assert!(sandbox
            .validate_tool_call("memfuse_insert", &Value::Null)
            .is_err());
        assert!(sandbox
            .validate_tool_call("memfuse_delete", &Value::Null)
            .is_err());
        assert!(sandbox
            .validate_tool_call("unknown_code_tool", &Value::Null)
            .is_err());
    }

    #[test]
    fn test_sandbox_permitting_policy() {
        let policy = SandboxPolicy {
            allow_db_reads: true,
            allow_db_writes: true,
            allow_code_execution: true,
            max_execution_ms: 5_000,
        };
        let sandbox = McpSandbox::new(policy);

        assert!(sandbox
            .validate_tool_call("memfuse_search", &Value::Null)
            .is_ok());
        assert!(sandbox
            .validate_tool_call("memfuse_insert", &Value::Null)
            .is_ok());
        assert!(sandbox
            .validate_tool_call("some_custom_tool", &Value::Null)
            .is_ok());
    }

    #[test]
    fn test_volatile_result_encryption_roundtrip() {
        let sandbox = McpSandbox::new(SandboxPolicy::default());
        let data = b"Top secret volatile tool result data";

        sandbox.store_volatile("res1", data).expect("store"); // expect

        let retrieved = sandbox.get_volatile("res1").expect("get").expect("exists"); // expect
        assert_eq!(retrieved, data);
    }

    #[test]
    fn volatile_tool_result_roundtrip() {
        let key =
            memfuse_crypto::CryptoKey::try_new("0123456789abcdef0123456789abcdef", b"salt1234")
                .unwrap();
        let plaintext = b"tool output data";
        let result = VolatileToolResult::encrypt(plaintext, &key).unwrap();
        let decrypted = result.decrypt(&key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_sandbox_execution_timeout() {
        let policy = SandboxPolicy {
            allow_db_reads: true,
            allow_db_writes: true,
            allow_code_execution: true,
            max_execution_ms: 50,
        };
        let sandbox = McpSandbox::new(policy);

        let slow_fut = async {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            Ok::<(), MemFuseError>(())
        };

        let res = sandbox.execute_with_timeout("slow_tool", slow_fut).await;
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("slow_tool"));
        assert!(err_msg.contains("überschritt Timeout 50ms"));
    }
}
