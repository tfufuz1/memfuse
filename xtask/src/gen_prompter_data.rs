use crate::{find_root_dir, get_workspace_crates};
use chrono::{NaiveDate, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Deserialize, Default)]
struct PrompterTiers {
    #[serde(default)]
    crate_overrides: BTreeMap<String, CrateOverride>,
    #[serde(default)]
    target_architecture: BTreeMap<String, String>,
    #[serde(default)]
    component_focus: BTreeMap<String, BTreeMap<String, ComponentFocus>>,
}

#[derive(Debug, Deserialize, Default)]
struct CrateOverride {
    tier: Option<String>,
    risk: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ComponentFocus {
    desc: Option<String>,
    focus: Option<String>,
    risk: Option<String>,
}

#[derive(Debug, Serialize)]
struct PrompterDataOutput {
    generated_at: String,
    head: String,
    target_architecture: BTreeMap<String, String>,
    crates: Vec<CrateJsonData>,
    components: BTreeMap<String, Vec<ComponentJsonData>>,
    working_state_snapshot: WorkingStateSnapshot,
}

#[derive(Debug, Serialize)]
struct WorkingStateSnapshot {
    generated_at: String,
    crate_count: usize,
    crates: Vec<WorkingStateCrate>,
    last_merge: String,
}

#[derive(Debug, Serialize)]
struct WorkingStateCrate {
    name: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct CrateJsonData {
    id: String,
    layer: u8,
    order: usize,
    loc: usize,
    tests: String,
    desc: String,
    status: String,
    risk: String,
    tier: String,
}

#[derive(Debug, Serialize)]
struct ComponentJsonData {
    file: String,
    size: String,
    loc: usize,
    risk: String,
    desc: String,
    focus: String,
}

fn determine_size_class(loc: usize) -> &'static str {
    if loc < 200 {
        "S"
    } else if loc <= 600 {
        "M"
    } else if loc <= 1500 {
        "L"
    } else {
        "XL"
    }
}

fn count_crate_tests(crate_path: &Path) -> usize {
    let mut count = 0;
    for sub in &["src", "tests"] {
        let dir = crate_path.join(sub);
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(&dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
                if let Ok(content) = fs::read_to_string(path) {
                    for line in content.lines() {
                        if line.contains("#[test]") || line.contains("#[tokio::test]") {
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    count
}

fn get_git_head_short() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn derive_status_from_working_state(crate_id: &str, ws_content: &str) -> String {
    for line in ws_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("| `") {
            let parts: Vec<&str> = trimmed.split('|').collect();
            if parts.len() >= 5 {
                let name = parts[1].trim().trim_matches('`');
                if name == crate_id {
                    let status_col = parts[4].trim();
                    if status_col.starts_with('🟢') {
                        return "✅".to_string();
                    } else if status_col.starts_with('🟡') {
                        return "🟡".to_string();
                    } else if status_col.starts_with('🔴') {
                        return "🔴".to_string();
                    } else if status_col.starts_with('🧊') {
                        return "🧊".to_string();
                    } else if status_col.starts_with('✅') {
                        return "✅".to_string();
                    } else {
                        return "❓".to_string();
                    }
                }
            }
        }
    }
    "❓".to_string()
}

pub fn run() -> bool {
    println!("=== Running xtask gen-prompter-data ===");
    let root = find_root_dir();

    // 1. Load .jules/prompter-tiers.toml
    let tiers_path = root.join(".jules/prompter-tiers.toml");
    let tiers_cfg: PrompterTiers = if tiers_path.exists() {
        match fs::read_to_string(&tiers_path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("Warning: Failed to parse {}: {}", tiers_path.display(), e);
                PrompterTiers::default()
            }),
            Err(e) => {
                eprintln!("Warning: Failed to read {}: {}", tiers_path.display(), e);
                PrompterTiers::default()
            }
        }
    } else {
        eprintln!("Warning: {} does not exist", tiers_path.display());
        PrompterTiers::default()
    };

    // 2. Get workspace crates (excluding xtask and benchmarks)
    let all_workspace_crates = get_workspace_crates();
    let workspace_crates: Vec<_> = all_workspace_crates
        .into_iter()
        .filter(|c| c.path.starts_with("crates/") || c.path.starts_with("benchmarks/"))
        .collect();

    let mut crates_json = Vec::new();
    let mut components_json = BTreeMap::new();

    for (idx, c) in workspace_crates.iter().enumerate() {
        let crate_id = &c.name;
        let crate_dir = root.join(&c.path);

        // Calculate test count
        let test_count = count_crate_tests(&crate_dir);
        let tests_str = format!("{}+", test_count);

        // Get tier & risk override
        let override_info = tiers_cfg.crate_overrides.get(crate_id);
        let tier = override_info
            .and_then(|o| o.tier.clone())
            .unwrap_or_else(|| "2".to_string());
        let risk = override_info
            .and_then(|o| o.risk.clone())
            .unwrap_or_else(|| "none".to_string());

        // Status
        let ws_path = root.join("WORKING_STATE.md");
        let ws_content = fs::read_to_string(&ws_path).unwrap_or_default();
        let status = derive_status_from_working_state(crate_id, &ws_content);

        crates_json.push(CrateJsonData {
            id: crate_id.clone(),
            layer: c.layer,
            order: idx + 1,
            loc: c.loc,
            tests: tests_str,
            desc: c.description.clone(),
            status,
            risk,
            tier,
        });

        // Collect components in src/
        let src_dir = crate_dir.join("src");
        let mut crate_components = Vec::new();

        if src_dir.exists() {
            let mut src_files = Vec::new();
            for entry in WalkDir::new(&src_dir).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
                    if let Ok(rel) = path.strip_prefix(&src_dir) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        src_files.push((rel_str, path.to_path_buf()));
                    }
                }
            }
            src_files.sort_by(|a, b| a.0.cmp(&b.0));

            let crate_focus_map = tiers_cfg.component_focus.get(crate_id);

            for (rel_file, full_path) in src_files {
                let loc = if let Ok(content) = fs::read_to_string(&full_path) {
                    content.lines().count()
                } else {
                    0
                };
                let size_class = determine_size_class(loc).to_string();

                // Lookup overlay
                let mut focus_data = None;
                if let Some(focus_map) = crate_focus_map {
                    if let Some(item) = focus_map.get(&rel_file) {
                        focus_data = Some(item);
                    } else {
                        // Check combo keys e.g. "error.rs / error_dto.rs"
                        for (k, item) in focus_map {
                            if k.split('/')
                                .map(|s| s.trim())
                                .any(|part| part == rel_file || k.contains(&rel_file))
                            {
                                focus_data = Some(item);
                                break;
                            }
                        }
                    }
                }

                let comp_risk = focus_data
                    .and_then(|d| d.risk.clone())
                    .unwrap_or_else(|| "—".to_string());
                let comp_desc = focus_data.and_then(|d| d.desc.clone()).unwrap_or_default();
                let comp_focus = focus_data.and_then(|d| d.focus.clone()).unwrap_or_default();

                crate_components.push(ComponentJsonData {
                    file: rel_file,
                    size: size_class,
                    loc,
                    risk: comp_risk,
                    desc: comp_desc,
                    focus: comp_focus,
                });
            }
        }

        components_json.insert(crate_id.clone(), crate_components);
    }

    // Read and parse WORKING_STATE.md for working_state_snapshot
    let ws_path = root.join("WORKING_STATE.md");
    let mut ws_crates = Vec::new();
    let mut last_merge = "unbekannt".to_string();

    if let Ok(ws_content) = fs::read_to_string(&ws_path) {
        for line in ws_content.lines() {
            let trimmed = line.trim();
            if trimmed.contains("Letzter Merge:") {
                let extracted = if let Some(idx) = trimmed.find("Letzter Merge:") {
                    trimmed[idx..].trim().to_string()
                } else {
                    trimmed.to_string()
                };
                if !extracted.is_empty() {
                    last_merge = extracted;
                }
            } else if trimmed.starts_with("| `memfuse-") {
                let parts: Vec<&str> = trimmed.split('|').collect();
                if parts.len() >= 5 {
                    let name = parts[1].trim().trim_matches('`').to_string();
                    let status = parts[4].trim().to_string();
                    ws_crates.push(WorkingStateCrate { name, status });
                }
            }
        }
    }

    let snapshot_now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let crate_count = ws_crates.len();

    let output_data = PrompterDataOutput {
        generated_at: snapshot_now.clone(),
        head: get_git_head_short(),
        target_architecture: tiers_cfg.target_architecture,
        crates: crates_json,
        components: components_json,
        working_state_snapshot: WorkingStateSnapshot {
            generated_at: snapshot_now,
            crate_count,
            crates: ws_crates,
            last_merge,
        },
    };

    let output_path = root.join(".jules/prompter-data.json");
    let json_str = match serde_json::to_string_pretty(&output_data) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to serialize prompter data: {}", e);
            return false;
        }
    };

    if let Err(e) = fs::write(&output_path, json_str) {
        eprintln!("❌ Failed to write {}: {}", output_path.display(), e);
        return false;
    }

    let total_components: usize = output_data.components.values().map(|v| v.len()).sum();
    println!(
        "✅ Generated {} with {} crates and {} component files.",
        output_path.display(),
        output_data.crates.len(),
        total_components
    );

    // 3. AGENTS.md timestamp freshness check
    let agents_path = root.join("AGENTS.md");
    if let Ok(agents_content) = fs::read_to_string(&agents_path) {
        let re_stand = Regex::new(r"Stand\s*(\d{4}-\d{2}-\d{2})").unwrap();
        if let Some(caps) = re_stand.captures(&agents_content) {
            if let Ok(agents_date) = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d") {
                let last_code_change_str = Command::new("git")
                    .args(["log", "-1", "--format=%cs", "--", "crates/"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();

                if let Ok(code_date) = NaiveDate::parse_from_str(&last_code_change_str, "%Y-%m-%d")
                {
                    let days_diff = (code_date - agents_date).num_days();
                    if days_diff > 3 {
                        eprintln!(
                            "⚠️  WORKING_STATE.md ist frisch, aber AGENTS.md Stand-Datum ist {} Tage alt (letzter Code-Change: {}). Erwäge 'just sync-agents'.",
                            days_diff, last_code_change_str
                        );
                    }
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_status_from_working_state() {
        let sample_ws = r#"
## Crate-Inventar & Status

| Crate | Layer | LOC | Status | Beschreibung / Hauptaufgabe |
| :--- | :---: | :---: | :--- | :--- |
| `memfuse-core` | 1 | 9364 | 🟢 Clean | Core types |
| `memfuse-embed` | 4 | 2033 | 🧊 Optional | Embedder |
| `memfuse-custom` | 5 | 1000 | 🟡 Warning | Custom |
| `memfuse-broken` | 6 | 500 | 🔴 Critical | Broken |
| `memfuse-unknown-status` | 7 | 100 | ⚪ Unknown | Unknown |
"#;

        assert_eq!(derive_status_from_working_state("memfuse-core", sample_ws), "✅");
        assert_eq!(derive_status_from_working_state("memfuse-embed", sample_ws), "🧊");
        assert_eq!(derive_status_from_working_state("memfuse-custom", sample_ws), "🟡");
        assert_eq!(derive_status_from_working_state("memfuse-broken", sample_ws), "🔴");
        assert_eq!(derive_status_from_working_state("memfuse-unknown-status", sample_ws), "❓");
        assert_eq!(derive_status_from_working_state("nonexistent-crate", sample_ws), "❓");
    }
}
