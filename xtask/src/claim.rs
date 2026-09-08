// MemFuse — Claim / Reservation Mechanism Gate
//
// Koordiniert parallele Agenten-Sessions, um Mehrfach-Implementierungen (z.B. ConfigFingerprint #1627, #1634, #1645)
// zu verhindern.
// Schema:
//   cargo xtask claim --crate <CRATE> --issue <TASK_ID> [--dry-run]

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::find_root_dir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimEntry {
    pub krate: String,
    pub issue: String,
    pub timestamp: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default = "default_active")]
    pub active: bool,
}

fn default_active() -> bool {
    true
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ClaimsDatabase {
    pub claims: Vec<ClaimEntry>,
}

impl ClaimsDatabase {
    pub fn load(path: &Path) -> Self {
        if path.is_file() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(db) = serde_json::from_str::<ClaimsDatabase>(&content) {
                    return db;
                }
            }
        }
        ClaimsDatabase { claims: Vec::new() }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Kann Verzeichnis {} nicht erstellen: {}",
                    parent.display(),
                    e
                )
            })?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Fehler bei Serialisierung der Claims: {}", e))?;
        fs::write(path, json)
            .map_err(|e| format!("Kann {} nicht schreiben: {}", path.display(), e))?;
        Ok(())
    }

    pub fn find_active_claim(&self, krate: &str) -> Option<&ClaimEntry> {
        self.claims.iter().find(|c| c.krate == krate && c.active)
    }
}

pub fn run_claim(args: &[String]) -> bool {
    if std::env::var("GITHUB_TOKEN")
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false)
    {
        return run_claim_github(args);
    }
    run_claim_local(args)
}

fn run_claim_local(args: &[String]) -> bool {
    let mut krate = String::new();
    let mut issue = String::new();
    let mut dry_run = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--crate" => {
                if i + 1 < args.len() {
                    krate = args[i + 1].clone();
                    i += 1;
                }
            }
            "--issue" => {
                if i + 1 < args.len() {
                    issue = args[i + 1].clone();
                    i += 1;
                }
            }
            "--dry-run" => {
                dry_run = true;
            }
            _ => {}
        }
        i += 1;
    }

    if krate.is_empty() {
        eprintln!("❌ Parameter --crate <CRATE> erforderlich.");
        return false;
    }
    if issue.is_empty() {
        issue = "UNSPECIFIED".to_string();
    }

    let root = find_root_dir();
    let claims_path = root.join(".jules/claims.json");
    let mut db = ClaimsDatabase::load(&claims_path);

    if let Some(existing) = db.find_active_claim(&krate) {
        if existing.issue != issue {
            eprintln!(
                "⚠️ KONFLIKT: Crate '{}' ist bereits aktiv reserviert durch Issue '{}' (seit {}).",
                krate, existing.issue, existing.timestamp
            );
            if !dry_run {
                eprintln!(
                    "Bitte warten oder mit Entwickler abstimmen, um doppelte Arbeit zu verhindern."
                );
                return false;
            }
        } else {
            println!(
                "ℹ️ Crate '{}' ist bereits für Issue '{}' beansprucht.",
                krate, issue
            );
            return true;
        }
    }

    let entry = ClaimEntry {
        krate: krate.clone(),
        issue: issue.clone(),
        timestamp: Utc::now().to_rfc3339(),
        session_id: std::env::var("JULES_SESSION_ID").unwrap_or_else(|_| "local".to_string()),
        active: true,
    };

    if dry_run {
        println!(
            "{{\"status\": \"claimed\", \"crate\": \"{}\", \"issue\": \"{}\", \"dry_run\": true}}",
            krate, issue
        );
        return true;
    }

    db.claims.push(entry);
    if let Err(e) = db.save(&claims_path) {
        eprintln!("❌ Fehler beim Speichern des Claims: {}", e);
        return false;
    }

    println!(
        "✅ Claim erfolgreich registriert: Crate '{}' für Issue '{}' gesperrt.",
        krate, issue
    );
    true
}

pub fn run_claim_github(args: &[String]) -> bool {
    let token = match std::env::var("GITHUB_TOKEN") {
        Ok(t) if !t.trim().is_empty() => t,
        _ => return run_claim_local(args),
    };

    let mut krate = String::new();
    let mut issue = String::new();
    let mut session_hash = String::new();
    let mut dry_run = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--crate" => {
                if i + 1 < args.len() {
                    krate = args[i + 1].clone();
                    i += 1;
                }
            }
            "--issue" => {
                if i + 1 < args.len() {
                    issue = args[i + 1].clone();
                    i += 1;
                }
            }
            "--session" | "--session-hash" => {
                if i + 1 < args.len() {
                    session_hash = args[i + 1].clone();
                    i += 1;
                }
            }
            "--dry-run" => {
                dry_run = true;
            }
            _ => {}
        }
        i += 1;
    }

    if krate.is_empty() {
        eprintln!("❌ Parameter --crate <CRATE> erforderlich.");
        return false;
    }
    if issue.is_empty() {
        issue = "UNSPECIFIED".to_string();
    }
    if session_hash.is_empty() {
        session_hash = std::env::var("MEMFUSE_SESSION_HASH")
            .or_else(|_| std::env::var("JULES_SESSION_ID"))
            .unwrap_or_else(|_| "local".to_string());
    }

    // Step c: Check existing issues with label "claim:<crate>" via GitHub REST API
    let check_url = format!(
        "https://api.github.com/repos/tfufuz1/memfuse/issues?labels=claim:{}&state=open",
        krate
    );
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-H",
            &format!("Authorization: Bearer {}", token),
            "-H",
            "User-Agent: memfuse-xtask",
            "-H",
            "Accept: application/vnd.github+json",
            &check_url,
        ])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let body = String::from_utf8_lossy(&out.stdout);
            if let Ok(issues) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(arr) = issues.as_array() {
                    if !arr.is_empty() {
                        eprintln!(
                            "⚠️ KONFLIKT: Offenes Issue mit Label 'claim:{}' existiert bereits auf GitHub.",
                            krate
                        );
                        if !dry_run {
                            return false;
                        }
                    }
                }
            }
        }
    }

    if dry_run {
        println!(
            "{{\"status\": \"claimed\", \"crate\": \"{}\", \"issue\": \"{}\", \"mode\": \"github\", \"dry_run\": true}}",
            krate, issue
        );
        return true;
    }

    // Step b: Create GitHub issue via REST API
    let create_url = "https://api.github.com/repos/tfufuz1/memfuse/issues";
    let title = format!("CLAIM: {} — {}", krate, issue);
    let payload = serde_json::json!({
        "title": title,
        "body": session_hash,
        "labels": ["claimed", format!("claim:{}", krate)]
    });

    let payload_str = match serde_json::to_string(&payload) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Fehler bei JSON-Serialisierung für Issue-Erstellung: {}", e);
            return false;
        }
    };

    let post_output = std::process::Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            "-H",
            &format!("Authorization: Bearer {}", token),
            "-H",
            "User-Agent: memfuse-xtask",
            "-H",
            "Accept: application/vnd.github+json",
            "-d",
            &payload_str,
            create_url,
        ])
        .output();

    match post_output {
        Ok(out) if out.status.success() => {
            let response_body = String::from_utf8_lossy(&out.stdout);
            if let Ok(json_res) = serde_json::from_str::<serde_json::Value>(&response_body) {
                if json_res.get("number").is_some() {
                    println!(
                        "✅ GitHub Claim-Issue erfolgreich erstellt: Crate '{}' für Issue '{}' gesperrt.",
                        krate, issue
                    );
                    return true;
                }
            }
            eprintln!("❌ GitHub API Fehler bei Issue-Erstellung: {}", response_body);
            false
        }
        Ok(out) => {
            eprintln!(
                "❌ curl-Befehl fehlgeschlagen mit Status Code {}",
                out.status
            );
            false
        }
        Err(e) => {
            eprintln!("❌ Fehler beim Ausführen von curl: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_claims_db_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("claims.json");

        let mut db = ClaimsDatabase::default();
        db.claims.push(ClaimEntry {
            krate: "memfuse-router".to_string(),
            issue: "ADR-063".to_string(),
            timestamp: "2026-09-08T18:00:00Z".to_string(),
            session_id: "s1".to_string(),
            active: true,
        });

        db.save(&path).unwrap();

        let loaded = ClaimsDatabase::load(&path);
        assert_eq!(loaded.claims.len(), 1);
        assert_eq!(
            loaded.find_active_claim("memfuse-router").unwrap().issue,
            "ADR-063"
        );
        assert!(loaded.find_active_claim("memfuse-core").is_none());
    }

    #[test]
    fn test_claim_empty_crate_returns_false() {
        let args = vec!["--issue".to_string(), "ISSUE-1".to_string()];
        assert!(!run_claim(&args));
        assert!(!run_claim_github(&args));
    }

    #[test]
    fn test_claim_github_fallback_without_token() {
        // Ensure GITHUB_TOKEN is unset for fallback test
        std::env::remove_var("GITHUB_TOKEN");
        let args = vec![
            "--crate".to_string(),
            "memfuse-test-fallback".to_string(),
            "--issue".to_string(),
            "TEST-FB".to_string(),
            "--dry-run".to_string(),
        ];
        assert!(run_claim_github(&args));
    }
}
