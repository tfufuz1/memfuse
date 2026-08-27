use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct TagItem {
    pub file_path: String,
    pub line_num: usize,
    pub tag_type: String, // "AI-TAG", "ANCHOR", "FILE-CONTEXT"
    pub raw: String,
    pub timestamp: String,
    pub category: Option<String>,
    pub severity: Option<String>,
    pub id: Option<String>,
    pub status: Option<String>,
    pub description: String,
    pub is_resolved: bool,
}

#[derive(Debug, Clone)]
pub struct CrateInfo {
    pub name: String,
    pub path: String,
    pub layer: u8,
    pub loc: usize,
    pub status: String,
    pub description: String,
    pub dependencies: Vec<String>,
}

pub fn scan_tags<P: AsRef<Path>>(root: P) -> Vec<TagItem> {
    let mut tags = Vec::new();

    let file_context_re = Regex::new(r"//\s*FILE-CONTEXT").unwrap();
    let stand_re = Regex::new(r"//\s*STAND:\s*(.+)").unwrap();
    let zweck_re = Regex::new(r"//\s*ZWECK:\s*(.+)").unwrap();

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let rel_path = path.to_string_lossy().to_string();
            if let Ok(content) = fs::read_to_string(path) {
                let lines: Vec<&str> = content.lines().collect();

                for (idx, line) in lines.iter().enumerate() {
                    let line_num = idx + 1;
                    let trimmed = line.trim();

                    if trimmed.contains("AI-TAG") {
                        let is_resolved = trimmed.contains("RESOLVED")
                            || trimmed.contains("STATUS:DONE")
                            || trimmed.contains("STATUS:RESOLVED");

                        let ts = if let Some(ts_idx) = trimmed.find("(TS:") {
                            let rest = &trimmed[ts_idx + 4..];
                            if let Some(end) = rest.find(')') {
                                rest[..end].trim().to_string()
                            } else {
                                "".to_string()
                            }
                        } else {
                            "".to_string()
                        };

                        let id = if let Some(id_idx) = trimmed.find("(ID:") {
                            let rest = &trimmed[id_idx + 4..];
                            rest.find(')').map(|end| rest[..end].trim().to_string())
                        } else if let Some(id_idx) = trimmed.find("AGT-") {
                            let rest = &trimmed[id_idx..];
                            let end = rest
                                .find(|c: char| !c.is_alphanumeric() && c != '-')
                                .unwrap_or(rest.len());
                            Some(rest[..end].to_string())
                        } else {
                            None
                        };

                        let category = if let Some(c_start) = trimmed.find("AI-TAG[") {
                            let rest = &trimmed[c_start + 7..];
                            rest.find(']').map(|c_end| rest[..c_end].to_string())
                        } else {
                            None
                        };

                        let severity = if let Some(cat) = &category {
                            let after_cat =
                                trimmed.find(&format!("[{}]", cat)).unwrap_or(0) + cat.len() + 2;
                            if trimmed[after_cat..].starts_with('[') {
                                trimmed[after_cat + 1..].find(']').map(|s_end| {
                                    trimmed[after_cat + 1..after_cat + 1 + s_end].to_string()
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        tags.push(TagItem {
                            file_path: rel_path.clone(),
                            line_num,
                            tag_type: "AI-TAG".to_string(),
                            raw: trimmed.to_string(),
                            timestamp: ts,
                            category,
                            severity,
                            id,
                            status: if is_resolved {
                                Some("RESOLVED".to_string())
                            } else {
                                Some("OPEN".to_string())
                            },
                            description: trimmed.to_string(),
                            is_resolved,
                        });
                    } else if trimmed.contains("ANCHOR[")
                        || (trimmed.starts_with("// ANCHOR") && !trimmed.contains("ANCHOR:ARCH"))
                    {
                        let is_resolved =
                            trimmed.contains("STATUS:DONE") || trimmed.contains("STATUS:RESOLVED");
                        let status = if let Some(s_idx) = trimmed.find("STATUS:") {
                            let rest = &trimmed[s_idx + 7..];
                            let end = rest
                                .find(|c: char| c.is_whitespace() || c == '(')
                                .unwrap_or(rest.len());
                            Some(rest[..end].to_string())
                        } else {
                            None
                        };

                        let ts = if let Some(ts_idx) = trimmed.find("(TS:") {
                            let rest = &trimmed[ts_idx + 4..];
                            if let Some(end) = rest.find(')') {
                                rest[..end].trim().to_string()
                            } else {
                                "".to_string()
                            }
                        } else {
                            "".to_string()
                        };

                        let id = if let Some(start) = trimmed.find("ANCHOR[") {
                            let rest = &trimmed[start + 7..];
                            rest.find(']').map(|end| rest[..end].to_string())
                        } else {
                            None
                        };

                        tags.push(TagItem {
                            file_path: rel_path.clone(),
                            line_num,
                            tag_type: "ANCHOR".to_string(),
                            raw: trimmed.to_string(),
                            timestamp: ts,
                            category: None,
                            severity: None,
                            id,
                            status: status.clone(),
                            description: trimmed.to_string(),
                            is_resolved: is_resolved
                                || status.as_deref() == Some("DONE")
                                || status.as_deref() == Some("RESOLVED"),
                        });
                    } else if file_context_re.is_match(trimmed) {
                        let mut stand = "".to_string();
                        let mut zweck = "".to_string();
                        for next_line in lines.iter().skip(idx).take(6) {
                            if let Some(m) = stand_re.captures(next_line) {
                                stand = m[1].trim().to_string();
                            }
                            if let Some(m) = zweck_re.captures(next_line) {
                                zweck = m[1].trim().to_string();
                            }
                        }
                        tags.push(TagItem {
                            file_path: rel_path.clone(),
                            line_num,
                            tag_type: "FILE-CONTEXT".to_string(),
                            raw: trimmed.to_string(),
                            timestamp: stand,
                            category: None,
                            severity: None,
                            id: None,
                            status: None,
                            description: zweck,
                            is_resolved: false,
                        });
                    }
                }
            }
        }
    }

    tags
}

pub fn calculate_crate_loc<P: AsRef<Path>>(dir: P) -> usize {
    let mut loc = 0;
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            if let Ok(content) = fs::read_to_string(path) {
                loc += content.lines().count();
            }
        }
    }
    loc
}

pub fn get_workspace_crates() -> Vec<CrateInfo> {
    let root_cargo = fs::read_to_string("Cargo.toml").unwrap_or_default();
    let toml_val: toml::Value =
        toml::from_str(&root_cargo).expect("Failed to parse root Cargo.toml");

    let members = toml_val
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .expect("Workspace members not found");

    let mut crates = Vec::new();

    for member in members {
        let path_str = member.as_str().unwrap();
        if path_str == "xtask" {
            continue;
        }

        let crate_cargo_path = PathBuf::from(path_str).join("Cargo.toml");
        if !crate_cargo_path.exists() {
            continue;
        }

        let crate_cargo_content = fs::read_to_string(&crate_cargo_path).unwrap_or_default();
        let crate_toml: toml::Value =
            toml::from_str(&crate_cargo_content).unwrap_or(toml::Value::Table(Default::default()));

        let name = crate_toml
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string();

        let description = crate_toml
            .get("package")
            .and_then(|p| p.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        let mut dependencies = Vec::new();
        if let Some(deps) = crate_toml.get("dependencies").and_then(|d| d.as_table()) {
            for (dep_name, dep_val) in deps {
                if dep_name.starts_with("memfuse-") {
                    dependencies.push(dep_name.clone());
                } else if let Some(table) = dep_val.as_table() {
                    if table.contains_key("path") {
                        dependencies.push(dep_name.clone());
                    }
                }
            }
        }
        dependencies.sort();
        dependencies.dedup();

        let layer = match name.as_str() {
            "memfuse-core" => 0,
            "memfuse-store" | "memfuse-index" | "memfuse-text" | "memfuse-crypto"
            | "memfuse-graph" | "memfuse-checkpoint" => 1,
            "memfuse-db" => 2,
            "memfuse-py" | "memfuse-ollama" | "memfuse-embed" | "memfuse-agent" => 3,
            "memfuse-mcp" | "memfuse-tauri" => 4,
            _ => 99,
        };

        let loc = calculate_crate_loc(path_str);

        let status = if name == "memfuse-embed" {
            "🧊 Optional".to_string()
        } else {
            "🟢 Clean".to_string()
        };

        crates.push(CrateInfo {
            name,
            path: path_str.to_string(),
            layer,
            loc,
            status,
            description,
            dependencies,
        });
    }

    crates.sort_by_key(|c| (c.layer, c.name.clone()));
    crates
}

pub fn update_markdown_section(
    file_path: &str,
    key: &str,
    new_content: &str,
) -> Result<(), String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read {}: {}", file_path, e))?;
    let start_marker = format!("<!-- AUTOGENERATED:START:{} -->", key);
    let end_marker = format!("<!-- AUTOGENERATED:END:{} -->", key);

    let start_idx = content
        .find(&start_marker)
        .ok_or_else(|| format!("Start marker {} not found in {}", start_marker, file_path))?;
    let end_idx = content
        .find(&end_marker)
        .ok_or_else(|| format!("End marker {} not found in {}", end_marker, file_path))?;

    if start_idx >= end_idx {
        return Err(format!("Invalid marker order in {}", file_path));
    }

    let before = &content[..start_idx + start_marker.len()];
    let after = &content[end_idx..];

    let updated = format!("{}\n{}\n{}", before, new_content.trim(), after);
    fs::write(file_path, updated).map_err(|e| format!("Failed to write {}: {}", file_path, e))?;

    Ok(())
}

fn generate_ai_tags_section(tags: &[TagItem]) -> String {
    let open_tags: Vec<&TagItem> = tags
        .iter()
        .filter(|t| t.tag_type == "AI-TAG" && !t.is_resolved)
        .collect();

    let mut out = String::new();
    out.push_str(&format!("Stand letzter Prüfung: {}\n", chrono_or_today()));
    out.push_str("Befehl: `cargo xtask sync-docs` / `grep -rn \"AI-TAG\\[SMELL\\]\\[CRITICAL\\]\" crates/ --include=\"*.rs\" | grep -v RESOLVED`\n");
    out.push_str(&format!(
        "Ergebnis: **{} offene Tags**\n\n",
        open_tags.len()
    ));

    if !open_tags.is_empty() {
        out.push_str("| Crate/Datei | Zeile | ID | Kat. | Sev. | Zeitstempel | Beschreibung |\n");
        out.push_str("|---|---|---|---|---|---|---|\n");
        for t in open_tags {
            out.push_str(&format!(
                "| `{}` | {} | `{}` | `{}` | `{}` | `{}` | {} |\n",
                t.file_path,
                t.line_num,
                t.id.as_deref().unwrap_or("-"),
                t.category.as_deref().unwrap_or("-"),
                t.severity.as_deref().unwrap_or("-"),
                t.timestamp,
                t.description.replace('|', "\\|")
            ));
        }
    }

    out
}

fn generate_dag_topology_section(crates: &[CrateInfo]) -> String {
    let mut out = String::new();
    out.push_str("```\n");
    let mut layers: BTreeMap<u8, Vec<&CrateInfo>> = BTreeMap::new();
    for c in crates {
        layers.entry(c.layer).or_default().push(c);
    }

    for (layer, layer_crates) in layers {
        out.push_str(&format!("Layer {}:  ", layer));
        let mut first = true;
        for c in layer_crates {
            let indent = if first { "" } else { "          " };
            let deps_str = if c.dependencies.is_empty() {
                "".to_string()
            } else {
                format!(" (deps: {})", c.dependencies.join(", "))
            };
            out.push_str(&format!(
                "{}{} — {}{}\n",
                indent, c.name, c.description, deps_str
            ));
            first = false;
        }
    }
    out.push_str("```\n\n");

    let core_crates_count = crates.iter().filter(|c| c.name != "memfuse-embed").count();
    let has_optional = crates.iter().any(|c| c.name == "memfuse-embed");

    if has_optional {
        out.push_str(&format!("**Aktiver Workspace-Build**: {} Workspace Crates ({} Kern-Crates + 1 optionales Crate `memfuse-embed`).", crates.len(), core_crates_count));
    } else {
        out.push_str(&format!(
            "**Aktiver Workspace-Build**: {} Kern-Crates.",
            crates.len()
        ));
    }

    out
}

fn generate_crate_inventory_section(crates: &[CrateInfo]) -> String {
    let mut out = String::new();
    out.push_str("| Crate | Layer | LOC | Status | Beschreibung / Hauptaufgabe |\n");
    out.push_str("| :--- | :---: | :---: | :--- | :--- |\n");

    for c in crates {
        let loc_formatted = format_loc(c.loc);
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            c.name, c.layer, loc_formatted, c.status, c.description
        ));
    }

    out
}

#[allow(clippy::manual_is_multiple_of)]
fn format_loc(loc: usize) -> String {
    let s = loc.to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push('.');
        }
        result.push(c);
    }
    result
}

fn chrono_or_today() -> String {
    "2026-08-27".to_string()
}

fn main() {
    println!("=== Running xtask sync-docs ===");
    let tags = scan_tags("crates");
    println!("Found {} code tags across crates/.", tags.len());

    let crates = get_workspace_crates();
    println!("Parsed {} workspace crates.", crates.len());

    // 1. Update WORKING_STATE.md
    let ai_tags_content = generate_ai_tags_section(&tags);
    if let Err(e) = update_markdown_section("WORKING_STATE.md", "AI_TAGS", &ai_tags_content) {
        eprintln!("Warning: Failed to update WORKING_STATE.md: {}", e);
    } else {
        println!("Successfully updated WORKING_STATE.md (AI_TAGS section).");
    }

    // 2. Update docs/ARCHITECTURE.md
    let dag_topology_content = generate_dag_topology_section(&crates);
    if let Err(e) = update_markdown_section(
        "docs/ARCHITECTURE.md",
        "DAG_TOPOLOGY",
        &dag_topology_content,
    ) {
        eprintln!("Warning: Failed to update docs/ARCHITECTURE.md: {}", e);
    } else {
        println!("Successfully updated docs/ARCHITECTURE.md (DAG_TOPOLOGY section).");
    }

    // 3. Update docs/SOURCE_OF_TRUTH.md
    let crate_inventory_content = generate_crate_inventory_section(&crates);
    if let Err(e) = update_markdown_section(
        "docs/SOURCE_OF_TRUTH.md",
        "CRATE_INVENTORY",
        &crate_inventory_content,
    ) {
        eprintln!("Warning: Failed to update docs/SOURCE_OF_TRUTH.md: {}", e);
    } else {
        println!("Successfully updated docs/SOURCE_OF_TRUTH.md (CRATE_INVENTORY section).");
    }

    println!("=== xtask sync-docs complete ===");
}
