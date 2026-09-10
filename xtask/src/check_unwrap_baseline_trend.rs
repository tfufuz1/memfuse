// xtask/src/check_unwrap_baseline_trend.rs
//
// Gate 2b: Monitor .unwrap() Baseline Trend
// Compares the current `.unwrap-baseline.json` with the baseline from the base branch (MEMFUSE_CI_BASE_REF / origin/main),
// computes per-crate diffs, reports Tier-1 net growth warnings (non-blocking design), and appends a history record.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::Command;

use crate::UnwrapBaselineEntry;

pub const DEFAULT_TIER1_CRATES: &[&str] = &[
    "memfuse-core",
    "memfuse-crypto",
    "memfuse-store",
    "memfuse-index",
];

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnwrapBaselineHistoryEntry {
    pub date: String,
    pub commit: String,
    pub total: usize,
    pub by_tier1_crate: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateDiff {
    pub crate_name: String,
    pub added: usize,
    pub removed: usize,
    pub net: i64,
}

/// Extract crate or top-level module name from file path.
/// Examples:
///   "crates/memfuse-core/src/lib.rs" -> "memfuse-core"
///   "xtask/src/main.rs" -> "xtask"
///   "benchmarks/memfuse-bench/src/lib.rs" -> "memfuse-bench"
///   "tests/foo.rs" -> "tests"
pub fn extract_crate_name(file_path: &str) -> String {
    let path = Path::new(file_path);
    let components: Vec<&str> = path
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect();

    if components.len() >= 2 && components[0] == "crates" {
        components[1].to_string()
    } else if components.len() >= 2 && components[0] == "benchmarks" {
        components[1].to_string()
    } else if !components.is_empty() {
        components[0].to_string()
    } else {
        "unknown".to_string()
    }
}

/// Read Tier 1 crates from prompter-tiers.toml if available, otherwise return default list.
pub fn load_tier1_crates(root: &Path) -> Vec<String> {
    let tiers_file = root.join(".jules/prompter-tiers.toml");
    if tiers_file.exists() {
        if let Ok(content) = fs::read_to_string(&tiers_file) {
            if let Ok(value) = content.parse::<toml::Value>() {
                if let Some(overrides) = value.get("crate_overrides").and_then(|v| v.as_table()) {
                    let mut tier1 = Vec::new();
                    for (crate_name, crate_table) in overrides {
                        if let Some(tier) = crate_table.get("tier").and_then(|t| t.as_str()) {
                            if tier == "1" {
                                tier1.push(crate_name.clone());
                            }
                        }
                    }
                    if !tier1.is_empty() {
                        tier1.sort();
                        return tier1;
                    }
                }
            }
        }
    }

    DEFAULT_TIER1_CRATES.iter().map(|s| s.to_string()).collect()
}

/// Parse a raw JSON string into a Set of UnwrapBaselineEntry
pub fn parse_baseline_entries(json_str: &str) -> Result<Vec<UnwrapBaselineEntry>, String> {
    serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {e}"))
}

/// Computes added, removed, and per-crate breakdown between base and current baselines.
pub fn compute_baseline_diff(
    base_entries: &[UnwrapBaselineEntry],
    current_entries: &[UnwrapBaselineEntry],
) -> (
    Vec<UnwrapBaselineEntry>,
    Vec<UnwrapBaselineEntry>,
    Vec<CrateDiff>,
) {
    let base_set: HashSet<(&str, &str)> = base_entries
        .iter()
        .map(|e| (e.file.as_str(), e.hash.as_str()))
        .collect();

    let current_set: HashSet<(&str, &str)> = current_entries
        .iter()
        .map(|e| (e.file.as_str(), e.hash.as_str()))
        .collect();

    let added: Vec<UnwrapBaselineEntry> = current_entries
        .iter()
        .filter(|e| !base_set.contains(&(e.file.as_str(), e.hash.as_str())))
        .cloned()
        .collect();

    let removed: Vec<UnwrapBaselineEntry> = base_entries
        .iter()
        .filter(|e| !current_set.contains(&(e.file.as_str(), e.hash.as_str())))
        .cloned()
        .collect();

    let mut crate_stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for entry in &added {
        let crate_name = extract_crate_name(&entry.file);
        let stat = crate_stats.entry(crate_name).or_insert((0, 0));
        stat.0 += 1;
    }

    for entry in &removed {
        let crate_name = extract_crate_name(&entry.file);
        let stat = crate_stats.entry(crate_name).or_insert((0, 0));
        stat.1 += 1;
    }

    let diffs: Vec<CrateDiff> = crate_stats
        .into_iter()
        .map(|(crate_name, (add_cnt, rem_cnt))| CrateDiff {
            crate_name,
            added: add_cnt,
            removed: rem_cnt,
            net: add_cnt as i64 - rem_cnt as i64,
        })
        .collect();

    (added, removed, diffs)
}

/// Fetch `.unwrap-baseline.json` content from the base branch using git show.
pub fn fetch_base_baseline_content(base_ref: &str) -> Result<String, String> {
    let arg = format!("{}:.unwrap-baseline.json", base_ref);
    let output = Command::new("git")
        .args(["show", &arg])
        .output()
        .map_err(|e| format!("git show {arg} call failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git show {arg} returned non-zero: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Appends a new history entry into `docs/unwrap_baseline_history.jsonl`.
pub fn append_history_entry(
    root: &Path,
    current_entries: &[UnwrapBaselineEntry],
    tier1_crates: &[String],
) -> Result<(), String> {
    let docs_dir = root.join("docs");
    if !docs_dir.exists() {
        fs::create_dir_all(&docs_dir)
            .map_err(|e| format!("Failed to create docs directory: {e}"))?;
    }

    let history_file = docs_dir.join("unwrap_baseline_history.jsonl");

    let today = Utc::now().format("%Y-%m-%d").to_string();
    let commit_sha = match Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    };

    let mut by_tier1_crate: BTreeMap<String, usize> = BTreeMap::new();
    for t1 in tier1_crates {
        by_tier1_crate.insert(t1.clone(), 0);
    }

    for entry in current_entries {
        let crate_name = extract_crate_name(&entry.file);
        if by_tier1_crate.contains_key(&crate_name) {
            *by_tier1_crate.get_mut(&crate_name).unwrap() += 1;
        }
    }

    let record = UnwrapBaselineHistoryEntry {
        date: today,
        commit: commit_sha,
        total: current_entries.len(),
        by_tier1_crate,
    };

    let json_line = serde_json::to_string(&record)
        .map_err(|e| format!("Serialization error for history entry: {e}"))?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&history_file)
        .map_err(|e| {
            format!(
                "Failed to open history file {}: {e}",
                history_file.display()
            )
        })?;

    writeln!(file, "{}", json_line).map_err(|e| format!("Failed to write to history file: {e}"))?;

    println!("📜 Historie fortgeschrieben in {}", history_file.display());
    Ok(())
}

pub fn run_check_unwrap_baseline_trend(root: &Path) -> bool {
    println!("=== Running xtask check-unwrap-baseline-trend ===");

    let current_path = root.join(".unwrap-baseline.json");
    if !current_path.exists() {
        eprintln!("❌ .unwrap-baseline.json missing at root!");
        return false;
    }

    let current_content = match fs::read_to_string(&current_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌ Failed to read .unwrap-baseline.json: {}", e);
            return false;
        }
    };

    let current_entries = match parse_baseline_entries(&current_content) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("❌ Failed to parse current .unwrap-baseline.json: {}", e);
            return false;
        }
    };

    let base_ref = env::var("MEMFUSE_CI_BASE_REF")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "origin/main".to_string());

    let (base_entries, base_available) = match fetch_base_baseline_content(&base_ref) {
        Ok(content) => {
            match parse_baseline_entries(&content) {
                Ok(entries) => (entries, true),
                Err(e) => {
                    println!("⚠️ Base branch baseline parsing failed ({}), assuming empty base baseline.", e);
                    (Vec::new(), false)
                }
            }
        }
        Err(e) => {
            println!(
                "ℹ️ Base branch baseline unreadable via git ({}), skipping base comparison.",
                e
            );
            (Vec::new(), false)
        }
    };

    let tier1_crates = load_tier1_crates(root);

    println!(
        "Total .unwrap()/.expect() baseline entries on current branch: {}",
        current_entries.len()
    );

    if base_available {
        println!(
            "Base branch ref: {} (total entries: {})",
            base_ref,
            base_entries.len()
        );
        let (_added, _removed, diffs) = compute_baseline_diff(&base_entries, &current_entries);

        println!("\n--- Crate Trend Summary vs. {} ---", base_ref);
        if diffs.is_empty() {
            println!("No baseline changes across any crates.");
        } else {
            for diff in &diffs {
                let sign = if diff.net > 0 { "+" } else { "" };
                println!(
                    "  - {:<20}: +{} / -{} entries (Net: {}{})",
                    diff.crate_name, diff.added, diff.removed, sign, diff.net
                );
            }
        }

        let tier1_set: BTreeSet<&str> = tier1_crates.iter().map(|s| s.as_str()).collect();
        let mut tier1_added = 0usize;
        let mut tier1_removed = 0usize;

        for diff in &diffs {
            if tier1_set.contains(diff.crate_name.as_str()) {
                tier1_added += diff.added;
                tier1_removed += diff.removed;
            }
        }

        let tier1_net = tier1_added as i64 - tier1_removed as i64;
        println!(
            "\nTier-1 Crates Net Growth: +{} / -{} (Net: {}{})",
            tier1_added,
            tier1_removed,
            if tier1_net > 0 { "+" } else { "" },
            tier1_net
        );

        if tier1_net > 0 {
            println!(
                "\n⚠️  WARNSTUFE: Nettowachstum an .unwrap()/.expect() in Tier-1-Crates ({:?})!",
                tier1_crates
            );
            println!(
                "    Nettowachstum: +{} Einträge seit Base-Branch {}.",
                tier1_net, base_ref
            );
            println!("    Unwraps in Tier-1-Crates bergen hohes Risiko für Lock-Poisoning-Kaskaden und FFI-Panic-Instabilitäten.");
            println!("    Hinweis: Dieses Gate schlägt bewusst NICHT hart fehl, um bestehende Workflows nicht abrupt zu blockieren,");
            println!(
                "    aber bitte plane den Abbau im Sinne von docs/UNWRAP_REDUCTION_PLAN.md ein."
            );
        }
    } else {
        println!("ℹ️ Base branch baseline non-comparable. Reporting current branch counts only.");
    }

    if let Err(e) = append_history_entry(root, &current_entries, &tier1_crates) {
        eprintln!("⚠️ Failed to record history entry: {}", e);
    }

    println!("\n✅ Trend analysis completed successfully.");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_extract_crate_name() {
        assert_eq!(
            extract_crate_name("crates/memfuse-core/src/lib.rs"),
            "memfuse-core"
        );
        assert_eq!(
            extract_crate_name("crates/memfuse-crypto/src/anti_tamper.rs"),
            "memfuse-crypto"
        );
        assert_eq!(extract_crate_name("xtask/src/main.rs"), "xtask");
        assert_eq!(
            extract_crate_name("benchmarks/memfuse-bench/src/lib.rs"),
            "memfuse-bench"
        );
        assert_eq!(extract_crate_name("docs/README.md"), "docs");
    }

    #[test]
    fn test_compute_baseline_diff() {
        let base = vec![
            UnwrapBaselineEntry {
                file: "crates/memfuse-core/src/a.rs".to_string(),
                hash: "111".to_string(),
            },
            UnwrapBaselineEntry {
                file: "crates/memfuse-crypto/src/b.rs".to_string(),
                hash: "222".to_string(),
            },
        ];

        let current = vec![
            UnwrapBaselineEntry {
                file: "crates/memfuse-core/src/a.rs".to_string(),
                hash: "111".to_string(),
            },
            UnwrapBaselineEntry {
                file: "crates/memfuse-core/src/c.rs".to_string(),
                hash: "333".to_string(),
            },
        ];

        let (added, removed, diffs) = compute_baseline_diff(&base, &current);

        assert_eq!(added.len(), 1);
        assert_eq!(added[0].file, "crates/memfuse-core/src/c.rs");

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].file, "crates/memfuse-crypto/src/b.rs");

        assert_eq!(diffs.len(), 2);

        let core_diff = diffs
            .iter()
            .find(|d| d.crate_name == "memfuse-core")
            .unwrap();
        assert_eq!(core_diff.added, 1);
        assert_eq!(core_diff.removed, 0);
        assert_eq!(core_diff.net, 1);

        let crypto_diff = diffs
            .iter()
            .find(|d| d.crate_name == "memfuse-crypto")
            .unwrap();
        assert_eq!(crypto_diff.added, 0);
        assert_eq!(crypto_diff.removed, 1);
        assert_eq!(crypto_diff.net, -1);
    }

    #[test]
    fn test_append_history_entry() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let entries = vec![
            UnwrapBaselineEntry {
                file: "crates/memfuse-core/src/lib.rs".to_string(),
                hash: "abc".to_string(),
            },
            UnwrapBaselineEntry {
                file: "crates/memfuse-store/src/lsm.rs".to_string(),
                hash: "def".to_string(),
            },
        ];

        let tier1 = vec![
            "memfuse-core".to_string(),
            "memfuse-crypto".to_string(),
            "memfuse-store".to_string(),
            "memfuse-index".to_string(),
        ];

        assert!(append_history_entry(root, &entries, &tier1).is_ok());

        let history_file = root.join("docs").join("unwrap_baseline_history.jsonl");
        assert!(history_file.exists());

        let content = fs::read_to_string(&history_file).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let parsed: UnwrapBaselineHistoryEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.total, 2);
        assert_eq!(*parsed.by_tier1_crate.get("memfuse-core").unwrap(), 1);
        assert_eq!(*parsed.by_tier1_crate.get("memfuse-crypto").unwrap(), 0);
        assert_eq!(*parsed.by_tier1_crate.get("memfuse-store").unwrap(), 1);
    }
}
