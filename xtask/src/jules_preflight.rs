use crate::claim::{expire_stale_claims, ClaimsDatabase};
use crate::{
    check_duplicate_symbols, run_check_consistency, run_check_dag,
    run_check_jules_context_freshness, run_check_review_coverage, run_check_unwrap_baseline,
    run_sync_docs, run_validate_tags, scan_tags,
};
use regex::Regex;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, PartialEq, Eq)]
pub enum CheckResult {
    Pass,
    Fail(String),
    Skip(String),
}

fn get_changed_rs_files_from_git_diff() -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .output();

    let mut files = match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| s.ends_with(".rs") && Path::new(s).exists())
                .collect::<Vec<String>>()
        }
        _ => Vec::new(),
    };

    if files.is_empty() {
        let output_head1 = std::process::Command::new("git")
            .args(["diff", "--name-only", "HEAD~1"])
            .output();
        if let Ok(out) = output_head1 {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                files = stdout
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| s.ends_with(".rs") && Path::new(s).exists())
                    .collect();
            }
        }
    }

    Ok(files)
}

fn derive_crate_from_changed_files(files: &[String]) -> Option<String> {
    for file in files {
        if let Some(rest) = file.strip_prefix("crates/") {
            if let Some(crate_name) = rest.split('/').next() {
                if !crate_name.trim().is_empty() {
                    return Some(crate_name.trim().to_string());
                }
            }
        }
    }
    None
}

pub fn check_no_active_claim_conflict(root: &Path, target_crate: Option<&str>) -> CheckResult {
    let _ = expire_stale_claims(root);

    let krate = match target_crate {
        Some(c) if !c.trim().is_empty() => c.to_string(),
        _ => match std::env::var("MEMFUSE_CLAIM_CRATE") {
            Ok(c) if !c.trim().is_empty() => c,
            _ => {
                let changed_files = get_changed_rs_files_from_git_diff().unwrap_or_default();
                let has_crates_rs_changes = changed_files
                    .iter()
                    .any(|f| f.starts_with("crates/") && f.ends_with(".rs"));
                if !has_crates_rs_changes {
                    return CheckResult::Skip(
                        "Keine geänderten .rs-Dateien unter crates/ gefunden".to_string(),
                    );
                }
                match derive_crate_from_changed_files(&changed_files) {
                    Some(derived) => derived,
                    None => {
                        return CheckResult::Skip(
                            "Keine Crate aus geänderten .rs-Dateien ableitbar".to_string(),
                        );
                    }
                }
            }
        },
    };

    let is_ci = std::env::var("MEMFUSE_CI")
        .map(|v| v == "true")
        .unwrap_or(false);
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());

    if !is_ci {
        if let Some(tok) = token {
            let check_url = format!(
                "https://api.github.com/repos/tfufuz1/memfuse/issues?labels=claim:{}&state=open",
                krate
            );
            let output = std::process::Command::new("curl")
                .args([
                    "-s",
                    "-H",
                    &format!("Authorization: Bearer {}", tok),
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
                                return CheckResult::Fail(format!(
                                    "Aktiver Claim auf GitHub für Crate '{}' gefunden.",
                                    krate
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    let claims_path = root.join(".jules/claims.json");
    let db = ClaimsDatabase::load(&claims_path);
    if let Some(existing) = db.find_active_claim(&krate) {
        return CheckResult::Fail(format!(
            "Aktiver lokaler Claim für Crate '{}' (Issue '{}', seit {}).",
            krate, existing.issue, existing.timestamp
        ));
    }

    CheckResult::Pass
}

/// Gate-Prüfergebnis mit Name, Bestanden-Flag und optionaler Fehlerbeschreibung.
struct GateResult {
    name: String,
    passed: bool,
    detail: Option<String>,
}

/// Führt die lokalen CI-Gate-Prüfungen aus.
/// `fast_only = true`: Nur statische Regex-Gates (<10s Ziel).
/// `fast_only = false`: Inkl. fmt, clippy, sync-docs, review-coverage, consistency.
pub fn run_jules_preflight(fast_only: bool) -> bool {
    let total_start = Instant::now();
    let mode = if fast_only { "FAST" } else { "FULL" };
    println!("=== xtask jules-preflight ({}) ===", mode);

    let mut results: Vec<GateResult> = Vec::new();

    // ── Fast Gates (statische Prüfungen, kein Compiler-Durchlauf) ──────────

    // Gate 1: Ungelöste CRITICAL/BLOCKER AI-TAGs
    {
        let start = Instant::now();
        let re = Regex::new(r"AI-TAG\[[^\]]+\]\[(CRITICAL|BLOCKER)\]").unwrap();
        let mut violations = Vec::new();
        for entry in walkdir::WalkDir::new("crates")
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
                if let Ok(content) = fs::read_to_string(path) {
                    for (idx, line) in content.lines().enumerate() {
                        if re.is_match(line) && !line.contains("RESOLVED") {
                            violations.push(format!(
                                "  {}:{}: {}",
                                path.display(),
                                idx + 1,
                                line.trim()
                            ));
                        }
                    }
                }
            }
        }
        let passed = violations.is_empty();
        let detail = if passed {
            None
        } else {
            Some(format!(
                "{} ungelöste CRITICAL/BLOCKER Tags:\n{}",
                violations.len(),
                violations.join("\n")
            ))
        };
        results.push(GateResult {
            name: format!(
                "Gate 1: Kritische AI-TAGs ({:.1}s)",
                start.elapsed().as_secs_f64()
            ),
            passed,
            detail,
        });
    }

    // Gate: Claim Conflict Check
    {
        let start = Instant::now();
        let root = crate::find_root_dir();
        let claim_res = check_no_active_claim_conflict(&root, None);
        let (passed, detail) = match claim_res {
            CheckResult::Pass => (true, None),
            CheckResult::Skip(reason) => (true, Some(format!("Übersprungen: {}", reason))),
            CheckResult::Fail(msg) => (false, Some(msg)),
        };
        results.push(GateResult {
            name: format!("Claim-Check ({:.1}s)", start.elapsed().as_secs_f64()),
            passed,
            detail,
        });
    }

    // Gate 2: Unwrap-Baseline
    {
        let start = Instant::now();
        let passed = run_check_unwrap_baseline();
        results.push(GateResult {
            name: format!(
                "Gate 2: Unwrap-Baseline ({:.1}s)",
                start.elapsed().as_secs_f64()
            ),
            passed,
            detail: None,
        });
    }

    // Gate 3: Silent IO-Fehler
    {
        let start = Instant::now();
        let re = Regex::new(r"let\s+_\s*=\s*.*(?:sync|flush|write)").unwrap();
        let mut violations = Vec::new();
        for entry in walkdir::WalkDir::new("crates")
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
                if let Ok(content) = fs::read_to_string(path) {
                    for (idx, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            violations.push(format!("  {}:{}", path.display(), idx + 1));
                        }
                    }
                }
            }
        }
        let passed = violations.is_empty();
        let detail = if passed {
            None
        } else {
            Some(format!(
                "Silent IO-Fehler gefunden:\n{}",
                violations.join("\n")
            ))
        };
        results.push(GateResult {
            name: format!("Gate 3: Silent IO ({:.1}s)", start.elapsed().as_secs_f64()),
            passed,
            detail,
        });
    }

    // Gate 4: axum in memfuse-mcp
    {
        let start = Instant::now();
        let mcp_cargo = fs::read_to_string("crates/memfuse-mcp/Cargo.toml").unwrap_or_default();
        let passed = !mcp_cargo.contains("axum");
        results.push(GateResult {
            name: format!(
                "Gate 4: Kein axum in MCP ({:.1}s)",
                start.elapsed().as_secs_f64()
            ),
            passed,
            detail: if passed {
                None
            } else {
                Some("axum-Dependency in memfuse-mcp — ADR-010 verletzt (stdio only)".to_string())
            },
        });
    }

    // Gate 6: TODOs ohne AI-TAG Grammatik
    {
        let start = Instant::now();
        let mut violations = Vec::new();
        for entry in walkdir::WalkDir::new("crates")
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
                if let Ok(content) = fs::read_to_string(path) {
                    for (idx, line) in content.lines().enumerate() {
                        if line.contains("TODO") && !line.contains("AI-TAG") {
                            violations.push(format!("  {}:{}", path.display(), idx + 1));
                        }
                    }
                }
            }
        }
        let passed = violations.is_empty();
        let detail = if passed {
            None
        } else {
            Some(format!(
                "{} TODOs ohne AI-TAG Grammatik:\n{}",
                violations.len(),
                violations
                    .iter()
                    .take(10)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        };
        results.push(GateResult {
            name: format!(
                "Gate 6: TODO-Grammatik ({:.1}s)",
                start.elapsed().as_secs_f64()
            ),
            passed,
            detail,
        });
    }

    // Gate 7: ISO-8601 Tag-Validierung
    {
        let start = Instant::now();
        let passed = run_validate_tags(false);
        results.push(GateResult {
            name: format!(
                "Gate 7: ISO-8601 Tags ({:.1}s)",
                start.elapsed().as_secs_f64()
            ),
            passed,
            detail: None,
        });
    }

    // Duplicate Symbols
    {
        let start = Instant::now();
        let changed_files = get_changed_rs_files_from_git_diff().unwrap_or_default();
        let dup_result = check_duplicate_symbols::check_duplicate_symbols(&changed_files);
        let passed = match &dup_result {
            Ok(dups) => dups.is_empty(),
            Err(_) => false,
        };
        let detail = match dup_result {
            Ok(dups) if !dups.is_empty() => Some(format!("{} Duplikate gefunden", dups.len())),
            Err(e) => Some(format!("Fehler: {}", e)),
            _ => None,
        };
        results.push(GateResult {
            name: format!("Duplicate Symbols ({:.1}s)", start.elapsed().as_secs_f64()),
            passed,
            detail,
        });
    }

    // DAG-Integrität
    {
        let start = Instant::now();
        let passed = run_check_dag();
        results.push(GateResult {
            name: format!("DAG-Integrität ({:.1}s)", start.elapsed().as_secs_f64()),
            passed,
            detail: None,
        });
    }

    // Gate: AGENTS.md Integrität
    {
        let start = Instant::now();
        let passed = crate::check_agents_integrity::run_check_agents_integrity();
        results.push(GateResult {
            name: format!(
                "AGENTS.md Integrität ({:.1}s)",
                start.elapsed().as_secs_f64()
            ),
            passed,
            detail: None,
        });
    }

    // ── Full Gates (Compiler-Durchlauf, dauert länger) ─────────────────────

    if !fast_only {
        // cargo fmt --check
        {
            let start = Instant::now();
            let output = std::process::Command::new("cargo")
                .args(["fmt", "--all", "--", "--check"])
                .output();
            let passed = output.map(|o| o.status.success()).unwrap_or(false);
            results.push(GateResult {
                name: format!("cargo fmt ({:.1}s)", start.elapsed().as_secs_f64()),
                passed,
                detail: if passed {
                    None
                } else {
                    Some("Formatierung nicht korrekt — `cargo fmt --all` ausführen".to_string())
                },
            });
        }

        // cargo clippy
        {
            let start = Instant::now();
            let output = std::process::Command::new("cargo")
                .args([
                    "clippy",
                    "--all-targets",
                    "--workspace",
                    "--exclude",
                    "memfuse-tauri",
                    "--",
                    "-D",
                    "warnings",
                ])
                .output();
            let passed = output.map(|o| o.status.success()).unwrap_or(false);
            results.push(GateResult {
                name: format!("cargo clippy ({:.1}s)", start.elapsed().as_secs_f64()),
                passed,
                detail: if passed {
                    None
                } else {
                    Some("Clippy-Warnungen gefunden".to_string())
                },
            });
        }

        // Gate 5: Dokumentations-Sync
        {
            let start = Instant::now();
            let passed = run_sync_docs(true);
            results.push(GateResult {
                name: format!("Gate 5: Docs-Sync ({:.1}s)", start.elapsed().as_secs_f64()),
                passed,
                detail: None,
            });
        }

        // Gate 8: Review-Coverage
        {
            let start = Instant::now();
            let tags = scan_tags("crates");
            let passed = run_check_review_coverage(&tags);
            results.push(GateResult {
                name: format!(
                    "Gate 8: Review-Coverage ({:.1}s)",
                    start.elapsed().as_secs_f64()
                ),
                passed,
                detail: None,
            });
        }

        // Gate 9: Konsistenzprüfung
        {
            let start = Instant::now();
            let passed = run_check_consistency();
            results.push(GateResult {
                name: format!("Gate 9: Konsistenz ({:.1}s)", start.elapsed().as_secs_f64()),
                passed,
                detail: None,
            });
        }

        // Gate 10: Jules Context Freshness
        {
            let start = Instant::now();
            let passed = run_check_jules_context_freshness();
            results.push(GateResult {
                name: format!(
                    "Gate 10: Jules Context ({:.1}s)",
                    start.elapsed().as_secs_f64()
                ),
                passed,
                detail: None,
            });
        }
    }

    // ── Summary ────────────────────────────────────────────────────────────

    println!();
    println!("┌───────────────────────────────────────────────────────────┐");
    println!(
        "│  Jules Preflight ({})  —  {:.1}s gesamt{}│",
        mode,
        total_start.elapsed().as_secs_f64(),
        " ".repeat(30 - mode.len() - format!("{:.1}", total_start.elapsed().as_secs_f64()).len())
    );
    println!("├───────────────────────────────────────────────────────────┤");

    let mut all_passed = true;
    for r in &results {
        let icon = if r.passed { "✅" } else { "❌" };
        println!("│  {} {}", icon, r.name);
        if let Some(detail) = &r.detail {
            for line in detail.lines().take(5) {
                println!("│     {}", line);
            }
        }
        if !r.passed {
            all_passed = false;
        }
    }

    println!("├───────────────────────────────────────────────────────────┤");
    let summary = if all_passed {
        "✅ ALLE GATES BESTANDEN — commit-ready"
    } else {
        "❌ GATES FEHLGESCHLAGEN — Fixes erforderlich"
    };
    println!("│  {}", summary);
    println!("└───────────────────────────────────────────────────────────┘");

    all_passed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::{ClaimEntry, ClaimsDatabase};
    use tempfile::tempdir;

    #[test]
    fn test_derive_crate_from_changed_files() {
        let files = vec![
            "README.md".to_string(),
            "crates/memfuse-core/src/lib.rs".to_string(),
            "crates/memfuse-db/src/lib.rs".to_string(),
        ];
        assert_eq!(
            derive_crate_from_changed_files(&files),
            Some("memfuse-core".to_string())
        );

        let non_crate_files = vec!["xtask/src/main.rs".to_string(), "docs/index.md".to_string()];
        assert_eq!(derive_crate_from_changed_files(&non_crate_files), None);
    }

    #[test]
    fn test_claim_conflict_check_unset_crate_skips() {
        std::env::remove_var("MEMFUSE_CLAIM_CRATE");
        let dir = tempdir().unwrap();
        let result = check_no_active_claim_conflict(dir.path(), None);
        // If git diff in test workspace has no crates/*.rs changes, it skips with "Keine geänderten .rs-Dateien unter crates/ gefunden"
        match result {
            CheckResult::Skip(reason) => {
                assert!(
                    reason.contains("crates/") || reason.contains("MEMFUSE_CLAIM_CRATE"),
                    "Unexpected skip reason: {}",
                    reason
                );
            }
            res => panic!("Expected Skip when no target crate/env var set and no crates/*.rs diff, got {:?}", res),
        }
    }

    #[test]
    fn test_claim_conflict_check_no_conflict_passes() {
        let dir = tempdir().unwrap();
        let result = check_no_active_claim_conflict(dir.path(), Some("memfuse-unclaimed"));
        assert_eq!(result, CheckResult::Pass);
    }

    #[test]
    fn test_claim_conflict_check_active_local_claim_fails() {
        let dir = tempdir().unwrap();
        let claims_dir = dir.path().join(".jules");
        fs::create_dir_all(&claims_dir).unwrap();
        let claims_path = claims_dir.join("claims.json");

        let mut db = ClaimsDatabase::default();
        db.claims.push(ClaimEntry {
            krate: "memfuse-active".to_string(),
            issue: "TASK-123".to_string(),
            timestamp: "2026-09-08T20:00:00Z".to_string(),
            session_id: "s123".to_string(),
            active: true,
            expires_at: None,
            released_at: None,
        });
        db.save(&claims_path).unwrap();

        let result = check_no_active_claim_conflict(dir.path(), Some("memfuse-active"));
        match result {
            CheckResult::Fail(msg) => {
                assert!(msg.contains("memfuse-active"));
                assert!(msg.contains("TASK-123"));
            }
            _ => panic!("Expected CheckResult::Fail for active local claim"),
        }
    }

    #[test]
    fn test_silent_io_regex() {
        let re = Regex::new(r"let\s+_\s*=\s*.*(?:sync|flush|write)").unwrap();
        assert!(re.is_match("    let _ = dir.sync_all();"));
        assert!(re.is_match("    let _ = file.flush().await;"));
        assert!(re.is_match("let _ = writer.write_all(&buf);"));
        assert!(!re.is_match("let result = dir.sync_all()?;"));
        assert!(!re.is_match("dir.sync_all().await?;"));
    }
}
