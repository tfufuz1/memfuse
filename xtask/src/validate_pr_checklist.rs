use regex::Regex;
use std::fs;
use std::path::Path;

use crate::{find_root_dir, run_check_dag};

/// Prüfergebnis pro Dimension.
struct DimensionResult {
    dimension: String,
    checks: Vec<CheckItem>,
}

struct CheckItem {
    label: String,
    status: CheckStatus,
    detail: Option<String>,
}

enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl CheckStatus {
    fn icon(&self) -> &str {
        match self {
            CheckStatus::Pass => "✅",
            CheckStatus::Warn => "⚠️ ",
            CheckStatus::Fail => "❌",
        }
    }
}

/// Validiert die PR-Checkliste gegen den aktuellen Git-Diff.
///
/// Führt heuristische Prüfungen durch, die der pull_request_template.md
/// entsprechen, und gibt eine formatierte Checkliste aus.
pub fn run_validate_pr_checklist() -> bool {
    let root = find_root_dir();
    println!("=== xtask validate-pr-checklist ===");

    // Git-Diff laden (staged + unstaged gegen HEAD)
    let diff = get_git_diff(&root);
    let diff_files = get_diff_files(&root);

    let mut dimensions: Vec<DimensionResult> = Vec::new();
    let mut has_failure = false;

    // ── Code-Dimension ─────────────────────────────────────────────────────

    let mut code_checks = Vec::new();

    // P1: DAG-Integrität
    let dag_ok = run_check_dag();
    code_checks.push(CheckItem {
        label: "DAG-Integrität (P1)".to_string(),
        status: if dag_ok {
            CheckStatus::Pass
        } else {
            has_failure = true;
            CheckStatus::Fail
        },
        detail: None,
    });

    // P2: unsafe ohne SAFETY
    let unsafe_violations = check_unsafe_without_safety(&diff);
    code_checks.push(CheckItem {
        label: "Kein unsafe ohne // SAFETY: (P2)".to_string(),
        status: if unsafe_violations.is_empty() {
            CheckStatus::Pass
        } else {
            has_failure = true;
            CheckStatus::Fail
        },
        detail: if unsafe_violations.is_empty() {
            None
        } else {
            Some(unsafe_violations.join("\n"))
        },
    });

    // P3: Silent IO
    let silent_io = check_silent_io_in_diff(&diff);
    code_checks.push(CheckItem {
        label: "Kein let _ = auf I/O (P3)".to_string(),
        status: if silent_io.is_empty() {
            CheckStatus::Pass
        } else {
            has_failure = true;
            CheckStatus::Fail
        },
        detail: if silent_io.is_empty() {
            None
        } else {
            Some(silent_io.join("\n"))
        },
    });

    dimensions.push(DimensionResult {
        dimension: "Code-Dimension (CI, Pflicht)".to_string(),
        checks: code_checks,
    });

    // ── Architektur-Dimension ──────────────────────────────────────────────

    let mut arch_checks = Vec::new();

    // P6: ADR bei architektonisch relevanten Änderungen
    let has_arch_changes = diff_files.iter().any(|f| {
        f.contains("Cargo.toml")
            || f.contains("traits.rs")
            || f.contains("error.rs")
            || f.ends_with("mod.rs")
    });
    let has_adr_changes = diff_files.iter().any(|f| f.contains("docs/decisions/"));

    if has_arch_changes && !has_adr_changes {
        arch_checks.push(CheckItem {
            label: "ADR bei architektonischen Änderungen (P6)".to_string(),
            status: CheckStatus::Warn,
            detail: Some(
                "Architektonisch relevante Dateien geändert (Cargo.toml/traits.rs/error.rs), aber kein ADR im Diff. Prüfe ob ein ADR erforderlich ist."
                    .to_string(),
            ),
        });
    } else {
        arch_checks.push(CheckItem {
            label: "ADR-Konsistenz (P6)".to_string(),
            status: CheckStatus::Pass,
            detail: None,
        });
    }

    // P10: Reuse-Check Hinweis
    let reuse_functions = ["score_batch", "tombstoned_edges", "persist_calibration_state"];
    let mut reuse_hints = Vec::new();
    for func in &reuse_functions {
        if diff.contains(func) {
            // Funktion wird im Diff referenziert — gut
        } else {
            // Prüfe ob ähnliche Logik im Diff vorkommt
            let has_scoring = diff.contains("score") && func == &"score_batch";
            let has_tombstone = diff.contains("tombstone") && func == &"tombstoned_edges";
            let has_calibration = diff.contains("calibrat") && func == &"persist_calibration_state";
            if has_scoring || has_tombstone || has_calibration {
                reuse_hints.push(format!("  Mögliche Wiederverwendung von {}()", func));
            }
        }
    }
    if !reuse_hints.is_empty() {
        arch_checks.push(CheckItem {
            label: "Reuse-Check (P10)".to_string(),
            status: CheckStatus::Warn,
            detail: Some(reuse_hints.join("\n")),
        });
    }

    dimensions.push(DimensionResult {
        dimension: "Architektur-Dimension".to_string(),
        checks: arch_checks,
    });

    // ── Provenienz-Dimension ───────────────────────────────────────────────

    let graph_changes = diff_files
        .iter()
        .any(|f| f.contains("memfuse-graph/"));
    if graph_changes {
        let has_provenance = diff.contains("EdgeProvenance");
        let mut prov_checks = Vec::new();
        prov_checks.push(CheckItem {
            label: "EdgeProvenance bei Graph-Änderungen (INV-GRAPH-PROV-1)".to_string(),
            status: if has_provenance {
                CheckStatus::Pass
            } else {
                CheckStatus::Warn
            },
            detail: if has_provenance {
                None
            } else {
                Some(
                    "Graph-Code geändert ohne EdgeProvenance-Referenz im Diff. Prüfe INV-GRAPH-PROV-1."
                        .to_string(),
                )
            },
        });
        dimensions.push(DimensionResult {
            dimension: "Provenienz-Dimension (Graph-Code)".to_string(),
            checks: prov_checks,
        });
    }

    // ── Nicht-Implementieren-Prüfung ───────────────────────────────────────

    let mut veto_checks = Vec::new();

    // VETO-01: Partieller HNSW-Rebuild
    let hnsw_changes = diff_files.iter().any(|f| {
        f.contains("memfuse-index/") && (f.contains("hnsw") || f.contains("rebuild"))
    });
    if hnsw_changes {
        let has_rebuild_region = diff.contains("rebuild_region");
        veto_checks.push(CheckItem {
            label: "VETO-01: Kein partieller HNSW-Rebuild".to_string(),
            status: if has_rebuild_region {
                has_failure = true;
                CheckStatus::Fail
            } else {
                CheckStatus::Warn
            },
            detail: Some(
                "HNSW-Code geändert. Prüfe ADR-071 / VETO-01 (physio-nucleation MUSS deaktiviert bleiben)."
                    .to_string(),
            ),
        });
    }

    // VETO-02: Cross-Tenant
    let cross_tenant_patterns = [
        "cross_tenant",
        "cross-tenant",
        "osmotic",
        "tenant_sharing",
        "merge_tenants",
    ];
    for pattern in &cross_tenant_patterns {
        if diff.to_lowercase().contains(pattern) {
            veto_checks.push(CheckItem {
                label: "VETO-02: Keine Cross-Tenant-Datenbewegung".to_string(),
                status: {
                    has_failure = true;
                    CheckStatus::Fail
                },
                detail: Some(format!(
                    "Pattern '{}' im Diff gefunden — verletzt VETO-02 (Mandantenisolation).",
                    pattern
                )),
            });
            break;
        }
    }

    if !veto_checks.is_empty() {
        dimensions.push(DimensionResult {
            dimension: "Nicht-Implementieren-Prüfung".to_string(),
            checks: veto_checks,
        });
    }

    // ── Chaos-Dimension (nur bei Storage-Änderungen) ───────────────────────

    let storage_changes = diff_files.iter().any(|f| {
        f.contains("memfuse-store/") || f.contains("wal") || f.contains("compaction")
    });
    if storage_changes {
        let mut chaos_checks = Vec::new();
        chaos_checks.push(CheckItem {
            label: "Power-Cut-Simulation für WAL/Compaction".to_string(),
            status: CheckStatus::Warn,
            detail: Some(
                "Storage-Code geändert. Prüfe ob Power-Cut-Tests (chaos_matrix) abgedeckt sind."
                    .to_string(),
            ),
        });

        let has_zeroize = diff.contains("Zeroize") || diff.contains("zeroize");
        let kv_cache_changes = diff.contains("kv_cache") || diff.contains("kv-cache");
        if kv_cache_changes && !has_zeroize {
            chaos_checks.push(CheckItem {
                label: "Zeroize-Nachweis bei KV-Cache-Code (P9)".to_string(),
                status: CheckStatus::Warn,
                detail: Some("KV-Cache-Code berührt ohne Zeroize-Referenz im Diff.".to_string()),
            });
        }

        dimensions.push(DimensionResult {
            dimension: "Chaos-Dimension (Storage-Änderungen)".to_string(),
            checks: chaos_checks,
        });
    }

    // ── Ausgabe ────────────────────────────────────────────────────────────

    println!();
    println!("┌───────────────────────────────────────────────────────────┐");
    println!("│  PR-Checkliste Validierung                               │");
    println!("├───────────────────────────────────────────────────────────┤");

    for dim in &dimensions {
        println!("│");
        println!("│  ### {}", dim.dimension);
        for check in &dim.checks {
            println!("│  {} {}", check.status.icon(), check.label);
            if let Some(detail) = &check.detail {
                for line in detail.lines().take(3) {
                    println!("│     {}", line);
                }
            }
        }
    }

    println!("│");
    println!("├───────────────────────────────────────────────────────────┤");
    if has_failure {
        println!("│  ❌ PR-Checkliste: KRITISCHE VERSTÖSSE GEFUNDEN          │");
    } else {
        println!("│  ✅ PR-Checkliste: Keine kritischen Verstöße             │");
    }
    println!("└───────────────────────────────────────────────────────────┘");

    !has_failure
}

/// Lädt den vollständigen Git-Diff (HEAD vs. Working Tree).
fn get_git_diff(root: &Path) -> String {
    // Versuche staged + unstaged diff
    let output = std::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(root)
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => {
            // Fallback: nur staged
            let output2 = std::process::Command::new("git")
                .args(["diff", "--cached"])
                .current_dir(root)
                .output();
            match output2 {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
                _ => String::new(),
            }
        }
    }
}

/// Listet die im Diff geänderten Dateien auf.
fn get_diff_files(root: &Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["diff", "HEAD", "--name-only"])
        .current_dir(root)
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Prüft auf `unsafe` Blöcke ohne `// SAFETY:` Kommentar in neuen Zeilen des Diffs.
fn check_unsafe_without_safety(diff: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let unsafe_re = Regex::new(r"^\+.*\bunsafe\b").unwrap();
    let safety_re = Regex::new(r"//\s*SAFETY:").unwrap();

    let mut current_file = String::new();
    let mut context_lines: Vec<String> = Vec::new();

    for line in diff.lines() {
        if line.starts_with("diff --git") {
            current_file = line
                .split(" b/")
                .last()
                .unwrap_or("")
                .to_string();
            context_lines.clear();
        } else if line.starts_with('+') && !line.starts_with("+++") {
            if unsafe_re.is_match(line) {
                // Prüfe ob SAFETY-Kommentar in den letzten 5 Kontext-Zeilen vorkommt
                let has_safety = context_lines
                    .iter()
                    .rev()
                    .take(5)
                    .any(|l| safety_re.is_match(l));

                // Oder ob der SAFETY-Kommentar in der gleichen Zeile steht
                let has_inline_safety = safety_re.is_match(line);

                if !has_safety && !has_inline_safety {
                    // Ignoriere Zeilen die nur Attribute oder Tests sind
                    let trimmed = line.trim_start_matches('+').trim();
                    if !trimmed.starts_with("//")
                        && !trimmed.starts_with("#[")
                        && !trimmed.contains("cfg(test)")
                        && !trimmed.contains("cfg_attr(not(test), forbid(unsafe_code))")
                    {
                        violations.push(format!(
                            "  {}: {}",
                            current_file,
                            trimmed
                        ));
                    }
                }
            }
            context_lines.push(line.to_string());
        } else {
            context_lines.push(line.to_string());
        }

        // Kontext auf 20 Zeilen begrenzen
        if context_lines.len() > 20 {
            context_lines.remove(0);
        }
    }

    violations
}

/// Prüft auf `let _ =` bei IO-Operationen in neuen Zeilen des Diffs.
fn check_silent_io_in_diff(diff: &str) -> Vec<String> {
    let re = Regex::new(r"let\s+_\s*=\s*.*(?:sync|flush|write)").unwrap();
    let mut violations = Vec::new();
    let mut current_file = String::new();

    for line in diff.lines() {
        if line.starts_with("diff --git") {
            current_file = line
                .split(" b/")
                .last()
                .unwrap_or("")
                .to_string();
        } else if line.starts_with('+') && !line.starts_with("+++") {
            if re.is_match(line) {
                violations.push(format!(
                    "  {}: {}",
                    current_file,
                    line.trim_start_matches('+').trim()
                ));
            }
        }
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_unsafe_without_safety() {
        let diff = r#"diff --git a/crates/test/src/lib.rs b/crates/test/src/lib.rs
--- a/crates/test/src/lib.rs
+++ b/crates/test/src/lib.rs
+    unsafe { ptr::copy(src, dst, len) }
"#;
        let violations = check_unsafe_without_safety(diff);
        assert!(!violations.is_empty(), "unsafe ohne SAFETY sollte erkannt werden");
    }

    #[test]
    fn test_check_unsafe_with_safety() {
        let diff = r#"diff --git a/crates/test/src/lib.rs b/crates/test/src/lib.rs
--- a/crates/test/src/lib.rs
+++ b/crates/test/src/lib.rs
+    // SAFETY: src and dst don't overlap
+    unsafe { ptr::copy(src, dst, len) }
"#;
        let violations = check_unsafe_without_safety(diff);
        assert!(violations.is_empty(), "unsafe MIT SAFETY sollte OK sein");
    }

    #[test]
    fn test_check_silent_io_in_diff() {
        let diff = r#"diff --git a/crates/test/src/lib.rs b/crates/test/src/lib.rs
+    let _ = dir.sync_all();
+    file.sync_all().await?;
"#;
        let violations = check_silent_io_in_diff(diff);
        assert_eq!(violations.len(), 1, "Nur die let _ = sync_all Zeile sollte matchen");
    }
}
