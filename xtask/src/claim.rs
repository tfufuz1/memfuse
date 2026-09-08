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
}
