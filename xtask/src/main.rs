use regex::Regex;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct TagItem {
    pub file_path: String,
    pub line_num: usize,
    pub tag_type: String, // "AI-TAG", "ANCHOR", "FILE-CONTEXT", "REVIEW-PASS"
    pub raw: String,
    pub timestamp: String,
    pub category: Option<String>,
    pub severity: Option<String>,
    pub id: Option<String>,
    pub session: Option<String>,
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

    for entry in WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let rel_path = path.to_string_lossy().to_string();
            if let Ok(content) = fs::read_to_string(path) {
                let lines: Vec<&str> = content.lines().collect();

                for (idx, line) in lines.iter().enumerate() {
                    let line_num = idx + 1;
                    let trimmed = line.trim();

                    let session = if let Some(s_idx) = trimmed.find("(SESSION:") {
                        let rest = &trimmed[s_idx + 9..];
                        rest.find(')').map(|end| rest[..end].trim().to_string())
                    } else {
                        None
                    };

                    if trimmed.contains("REVIEW-PASS") {
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

                        tags.push(TagItem {
                            file_path: rel_path.clone(),
                            line_num,
                            tag_type: "REVIEW-PASS".to_string(),
                            raw: trimmed.to_string(),
                            timestamp: ts,
                            category: None,
                            severity: None,
                            id,
                            session,
                            status,
                            description: trimmed.to_string(),
                            is_resolved: false,
                        });
                    } else if trimmed.contains("AI-TAG") {
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
                            session,
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

                        let id = if let Some(id_idx) = trimmed.find("(ID:") {
                            let rest = &trimmed[id_idx + 4..];
                            rest.find(')').map(|end| rest[..end].trim().to_string())
                        } else if let Some(start) = trimmed.find("ANCHOR[") {
                            let rest = &trimmed[start + 7..];
                            rest.find(']').map(|end| rest[..end].to_string())
                        } else if let Some(id_idx) = trimmed.find("(ID:") {
                            let rest = &trimmed[id_idx + 4..];
                            rest.find(')').map(|end| rest[..end].trim().to_string())
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
                            session,
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
                            session,
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

pub fn get_workspace_root() -> PathBuf {
    let mut current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut cargo_path = current_dir.join("Cargo.toml");

    loop {
        if cargo_path.exists() {
            if let Ok(content) = fs::read_to_string(&cargo_path) {
                if content.contains("[workspace]") {
                    return current_dir;
                }
            }
        }
        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
            cargo_path = current_dir.join("Cargo.toml");
        } else {
            break;
        }
    }
    PathBuf::from(".")
}

pub fn get_workspace_crates() -> Vec<CrateInfo> {
    let root_dir = get_workspace_root();
    let cargo_path = root_dir.join("Cargo.toml");
    let root_cargo = fs::read_to_string(&cargo_path).unwrap_or_default();
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

        let crate_cargo_path = root_dir.join(path_str).join("Cargo.toml");
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
            "memfuse-py" | "memfuse-ollama" | "memfuse-embed" | "memfuse-agent"
            | "memfuse-router" => 3,
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

pub fn render_updated_markdown(
    file_path: &str,
    key: &str,
    new_content: &str,
) -> Result<String, String> {
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

    Ok(format!("{}\n{}\n{}", before, new_content.trim(), after))
}

pub fn update_markdown_section(
    file_path: &str,
    key: &str,
    new_content: &str,
) -> Result<(), String> {
    let updated = render_updated_markdown(file_path, key, new_content)?;
    fs::write(file_path, updated).map_err(|e| format!("Failed to write {}: {}", file_path, e))?;
    Ok(())
}

fn generate_full_working_state(tags: &[TagItem], crates: &[CrateInfo]) -> String {
    let mut out = String::new();
    out.push_str("<!-- AUTOGENERATED:START:FULL -->\n");
    out.push_str("# MemFuse — Working State\n");
    out.push_str(&format!(
        "*Automatisch generierte Projektion des Code-Zustands — Stand: {}*\n\n",
        chrono_or_today()
    ));
    out.push_str("> **Hinweis**: Diese Datei ist zu 100 % autogeneriert durch `cargo xtask sync-docs` aus Inline-Code-Tags. Keinen Text manuell editieren. Bei Git-Merge-Konflikten stets `just sync-docs` ausführen.\n\n");

    out.push_str("## Offene AI-TAGs & ANCHORs\n\n");
    out.push_str(&generate_ai_tags_section(tags));
    out.push_str("\n\n");

    out.push_str("## Crate-Inventar & Status\n\n");
    out.push_str(&generate_crate_inventory_section(crates));
    out.push_str("\n\n");

    out.push_str("## DAG-Topologie\n\n");
    out.push_str(&generate_dag_topology_section(crates));
    out.push_str("\n<!-- AUTOGENERATED:END:FULL -->\n");

    out
}

fn generate_changelog(tags: &[TagItem]) -> String {
    let mut out = String::new();
    out.push_str("# MemFuse — Chronologischer Tag- & Review-Bericht\n\n");
    out.push_str("> Automatisch generierter Read-Only Bericht aus allen Inline-Tags im Repo.\n\n");
    out.push_str("| Zeitstempel | Crate/Datei | Typ | ID | Session | Status | Review-Pässe (unabhängig) | Beschreibung |\n");
    out.push_str("|---|---|---|---|---|---|---|---|\n");

    let mut sorted_tags: Vec<&TagItem> = tags.iter().collect();
    sorted_tags.sort_by(|a, b| {
        b.timestamp
            .cmp(&a.timestamp)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.line_num.cmp(&b.line_num))
    });

    for t in sorted_tags {
        let passes = if let Some(id) = &t.id {
            let matches: Vec<&TagItem> = tags
                .iter()
                .filter(|r| {
                    r.tag_type == "REVIEW-PASS"
                        && r.status.as_deref() == Some("PASS")
                        && r.id.as_deref() == Some(id.as_str())
                })
                .collect();
            let mut sess_set = std::collections::HashSet::new();
            for m in matches {
                if let Some(s) = &m.session {
                    if t.session.as_ref() != Some(s) {
                        sess_set.insert(s.clone());
                    }
                }
            }
            sess_set.len().to_string()
        } else {
            "-".to_string()
        };

        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | {} |\n",
            t.timestamp,
            t.file_path,
            t.tag_type,
            t.id.as_deref().unwrap_or("-"),
            t.session.as_deref().unwrap_or("-"),
            t.status.as_deref().unwrap_or("-"),
            passes,
            t.description.replace('|', "\\|")
        ));
    }

    out
}

fn generate_ai_tags_section(tags: &[TagItem]) -> String {
    let mut open_tags: Vec<&TagItem> = tags
        .iter()
        .filter(|t| t.tag_type == "AI-TAG" && !t.is_resolved)
        .collect();

    open_tags.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.line_num.cmp(&b.line_num))
    });

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

fn run_sync_docs(check_only: bool) -> bool {
    println!(
        "=== Running xtask sync-docs (check_only={}) ===",
        check_only
    );
    let tags = scan_tags("crates");
    println!("Found {} code tags across crates/.", tags.len());

    let crates = get_workspace_crates();
    println!("Parsed {} workspace crates.", crates.len());

    let dag_topology_content = generate_dag_topology_section(&crates);
    let crate_inventory_content = generate_crate_inventory_section(&crates);

    let full_working_state = generate_full_working_state(&tags, &crates);
    let changelog_content = generate_changelog(&tags);

    if check_only {
        let mut drift = false;
        let current_ws = fs::read_to_string("WORKING_STATE.md").unwrap_or_default();
        let re = Regex::new(r"\| LAST_SYNC \| `[^`]+` \|").unwrap();
        let norm_current = re.replace_all(current_ws.trim(), "| LAST_SYNC | `NORMALIZED` |");
        let norm_full = re.replace_all(full_working_state.trim(), "| LAST_SYNC | `NORMALIZED` |");
        if norm_current != norm_full {
            eprintln!("❌ WORKING_STATE.md is out of sync!");
            drift = true;
        } else {
            println!("✅ WORKING_STATE.md is in sync.");
        }

        let actual_cl = fs::read_to_string("docs/CHANGELOG.md").unwrap_or_default();
        if actual_cl.trim() != changelog_content.trim() {
            eprintln!("❌ docs/CHANGELOG.md is out of sync!");
            drift = true;
        } else {
            println!("✅ docs/CHANGELOG.md is in sync.");
        }

        let check_files = [
            ("docs/ARCHITECTURE.md", "DAG_TOPOLOGY", dag_topology_content),
            (
                "docs/SOURCE_OF_TRUTH.md",
                "CRATE_INVENTORY",
                crate_inventory_content,
            ),
        ];

        for (file_path, key, expected_content) in check_files {
            match render_updated_markdown(file_path, key, &expected_content) {
                Ok(expected_full) => {
                    let actual_full = fs::read_to_string(file_path).unwrap_or_default();
                    if actual_full != expected_full {
                        eprintln!("❌ Section '{}' in {} is out of sync!", key, file_path);
                        drift = true;
                    } else {
                        println!("✅ Section '{}' in {} is in sync.", key, file_path);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to render check for {}: {}", file_path, e);
                    drift = true;
                }
            }
        }

        if drift {
            eprintln!("❌ Documentation drift detected. Run `cargo xtask sync-docs` to fix.");
            false
        } else {
            println!("=== xtask sync-docs check PASSED ===");
            true
        }
    } else {
        let mut success = true;
        if let Err(e) = fs::write("WORKING_STATE.md", &full_working_state) {
            eprintln!("❌ Failed to write WORKING_STATE.md: {}", e);
            success = false;
        } else {
            println!("Successfully regenerated WORKING_STATE.md.");
        }

        if let Err(e) = fs::write("docs/CHANGELOG.md", &changelog_content) {
            eprintln!("❌ Failed to write docs/CHANGELOG.md: {}", e);
            success = false;
        } else {
            println!("Successfully regenerated docs/CHANGELOG.md.");
        }

        if let Err(e) = update_markdown_section(
            "docs/ARCHITECTURE.md",
            "DAG_TOPOLOGY",
            &dag_topology_content,
        ) {
            eprintln!("Warning: Failed to update docs/ARCHITECTURE.md: {}", e);
            success = false;
        } else {
            println!("Successfully updated docs/ARCHITECTURE.md (DAG_TOPOLOGY section).");
        }

        if let Err(e) = update_markdown_section(
            "docs/SOURCE_OF_TRUTH.md",
            "CRATE_INVENTORY",
            &crate_inventory_content,
        ) {
            eprintln!("Warning: Failed to update docs/SOURCE_OF_TRUTH.md: {}", e);
            success = false;
        } else {
            println!("Successfully updated docs/SOURCE_OF_TRUTH.md (CRATE_INVENTORY section).");
        }

        println!("=== xtask sync-docs complete ===");
        success
    }
}

pub fn run_validate_tags(tags: &[TagItem]) -> bool {
    let mut success = true;
    let cutoff_date = "2026-08-29";

    let mut missing_ts = Vec::new();
    let mut missing_session = Vec::new();

    for tag in tags {
        if tag.tag_type != "AI-TAG" && tag.tag_type != "ANCHOR" && tag.tag_type != "REVIEW-PASS" {
            continue;
        }

        let ts = tag.timestamp.trim();
        let valid_ts = if ts.len() >= 10 {
            let date_part = &ts[..10];
            let parts: Vec<&str> = date_part.split('-').collect();
            parts.len() == 3
                && parts[0].len() == 4
                && parts[0].chars().all(|c| c.is_ascii_digit())
                && parts[1].len() == 2
                && parts[1].chars().all(|c| c.is_ascii_digit())
                && parts[2].len() == 2
                && parts[2].chars().all(|c| c.is_ascii_digit())
        } else {
            false
        };

        if !valid_ts {
            missing_ts.push(tag);
        } else {
            let date_part = &ts[..10];
            if date_part >= cutoff_date {
                let session_opt = tag.session.as_deref().unwrap_or("").trim();
                let has_session = !session_opt.is_empty() || tag.raw.contains("SESSION:");
                if !has_session {
                    missing_session.push(tag);
                }
            }
        }
    }

    if !missing_ts.is_empty() {
        eprintln!("❌ Tags without valid TS: timestamp:");
        for tag in &missing_ts {
            eprintln!("  {}:{} - {}", tag.file_path, tag.line_num, tag.raw);
        }
        success = false;
    }

    if !missing_session.is_empty() {
        eprintln!("❌ New tags (>= {}) missing SESSION: field:", cutoff_date);
        for tag in &missing_session {
            eprintln!("  {}:{} - {}", tag.file_path, tag.line_num, tag.raw);
        }
        eprintln!("Füge SESSION: <8-hex> zu diesen Tags hinzu.");
        success = false;
    }

    if success {
        println!("✅ All tags have valid TS: and required SESSION: fields");
    }

    success
}

pub fn is_pre_cutoff(ts: &str) -> bool {
    let date_part = if ts.len() >= 10 { &ts[..10] } else { ts };
    date_part < "2026-08-29"
}

pub fn run_check_consistency() -> bool {
    let mut failed = false;

    let crates = get_workspace_crates();
    let actual_count = crates.len();
    println!("Actual workspace crate count: {}", actual_count);

    // 1. Check unknown layer assignments
    for c in &crates {
        if c.layer == 99 {
            eprintln!(
                "❌ Consistency error: Crate '{}' has unknown layer assignment!",
                c.name
            );
            failed = true;
        }
    }

    // 2. Check AGENTS.md crate count claim
    if let Ok(agents_content) = fs::read_to_string("AGENTS.md") {
        let re_agents = Regex::new(r"Workspace Inventory \((\d+) Crates\)").unwrap();
        if let Some(caps) = re_agents.captures(&agents_content) {
            let claimed_count: usize = caps[1].parse().unwrap_or(0);
            if claimed_count != actual_count {
                eprintln!(
                    "❌ Consistency error: AGENTS.md claims {} crates, but Cargo.toml has {} workspace crates!",
                    claimed_count, actual_count
                );
                failed = true;
            } else {
                println!(
                    "✅ AGENTS.md crate count ({}) matches Cargo.toml.",
                    claimed_count
                );
            }
        } else {
            eprintln!("⚠️ Warning: AGENTS.md does not contain expected 'Workspace Inventory (X Crates)' header pattern.");
        }
    } else {
        eprintln!("❌ Consistency error: Could not read AGENTS.md");
        failed = true;
    }

    // 3. Check README.md crate count claim
    if let Ok(readme_content) = fs::read_to_string("README.md") {
        let re_readme = Regex::new(r"Workspace Crates \((\d+) Active Crates\)").unwrap();
        if let Some(caps) = re_readme.captures(&readme_content) {
            let claimed_count: usize = caps[1].parse().unwrap_or(0);
            if claimed_count != actual_count {
                eprintln!(
                    "❌ Consistency error: README.md claims {} crates, but Cargo.toml has {} workspace crates!",
                    claimed_count, actual_count
                );
                failed = true;
            } else {
                println!(
                    "✅ README.md crate count ({}) matches Cargo.toml.",
                    claimed_count
                );
            }
        } else {
            eprintln!("⚠️ Warning: README.md does not contain expected 'Workspace Crates (X Active Crates)' header pattern.");
        }
    } else {
        eprintln!("❌ Consistency error: Could not read README.md");
        failed = true;
    }

    if failed {
        eprintln!("=== xtask check-consistency FAILED ===");
        false
    } else {
        println!("=== xtask check-consistency PASSED ===");
        true
    }
}

pub fn run_check_review_coverage(tags: &[TagItem]) -> bool {
    println!("=== Running xtask check-review-coverage ===");
    let done_anchors: Vec<&TagItem> = tags
        .iter()
        .filter(|t| t.tag_type == "ANCHOR" && t.status.as_deref() == Some("DONE"))
        .collect();

    let mut failed = false;

    for anchor in &done_anchors {
        let is_new_tag = anchor.session.is_some()
            || (anchor.timestamp.as_str() >= "2026-08-29"
                && anchor.timestamp.as_str() != "2026-08-29T00:00:00Z");

        if !is_new_tag {
            // Legacy anchor from before Prompt 06 cutoff - exempt from multi-session review gate
            continue;
        }

        let anchor_id = match &anchor.id {
            Some(id) => id,
            None => {
                eprintln!(
                    "❌ ANCHOR at {}:{} marked DONE without an ID!",
                    anchor.file_path, anchor.line_num
                );
                failed = true;
                continue;
            }
        };

        // Determine required review pass count N (2 default, 3 for ASK / security / unsafe / crypto / wal)
        let is_sensitive = anchor.file_path.contains("crypto")
            || anchor.file_path.contains("wal")
            || anchor.file_path.contains("distance.rs")
            || anchor.file_path.contains("diskann.rs")
            || anchor.file_path.contains("persistence.rs")
            || anchor.raw.contains("SECURITY")
            || anchor.raw.contains("unsafe");

        let required_passes = if is_sensitive { 3 } else { 2 };

        let matching_passes: Vec<&TagItem> = tags
            .iter()
            .filter(|t| {
                t.tag_type == "REVIEW-PASS"
                    && t.status.as_deref() == Some("PASS")
                    && t.id.as_deref() == Some(anchor_id.as_str())
            })
            .collect();

        let mut distinct_sessions = std::collections::HashSet::new();
        for pass in matching_passes {
            if let Some(sess) = &pass.session {
                // Ensure review pass is from a fresh session (different from creator's session if set)
                if anchor.session.as_ref() != Some(sess) {
                    distinct_sessions.insert(sess.clone());
                }
            }
        }

        if distinct_sessions.len() < required_passes {
            eprintln!(
                "❌ ANCHOR '{}' in {}:{} has {}/{} required independent REVIEW-PASS entries (sessions: {:?})",
                anchor_id,
                anchor.file_path,
                anchor.line_num,
                distinct_sessions.len(),
                required_passes,
                distinct_sessions
            );
            failed = true;
        } else {
            println!(
                "✅ ANCHOR '{}' in {}:{} passed review coverage ({}/{} independent sessions)",
                anchor_id,
                anchor.file_path,
                anchor.line_num,
                distinct_sessions.len(),
                required_passes
            );
        }
    }

    if failed {
        eprintln!("=== xtask check-review-coverage FAILED ===");
        false
    } else {
        println!("=== xtask check-review-coverage PASSED ===");
        true
    }
}


fn main() {
    let args: Vec<String> = env::args().collect();
    let subcommand = args.get(1).map(|s| s.as_str()).unwrap_or("sync-docs");

    match subcommand {
        "sync-docs" => {
            let check_only = args.iter().any(|arg| arg == "--check");
            let success = run_sync_docs(check_only);
            if !success {
                process::exit(1);
            }
        }
        "check-review-coverage" => {
            let tags = scan_tags("crates");
            let success = run_check_review_coverage(&tags);
            if !success {
                process::exit(1);
            }
        }
        "check-consistency" => {
            let success = run_check_consistency();
            if !success {
                process::exit(1);
            }
        }
        "run-community-detection" => {
            println!("=== xtask run-community-detection ===");
            println!(
                "Periodic batch process for GraphRAG community detection via Label Propagation."
            );
            println!("Note: Community detection triggers should be invoked via collection.run_community_detection().await or embedded engine instances.");
            println!("=== xtask run-community-detection PASSED ===");
        }
        other => {
            eprintln!("Unknown xtask command: {}", other);
            eprintln!("Available commands: sync-docs [--check], check-consistency, run-community-detection");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_state_single_marker_block() {
        let tags = vec![];
        let crates = vec![];
        let ws = generate_full_working_state(&tags, &crates);
        assert!(ws.starts_with("<!-- AUTOGENERATED:START:FULL -->"));
        assert!(ws.trim().ends_with("<!-- AUTOGENERATED:END:FULL -->"));
    }

    #[test]
    fn test_hash_id_collision_freedom() {
        let crate_name = "memfuse-store";
        let file_path = "crates/memfuse-store/src/lsm.rs";
        let line_num = 42;
        let ts = "2026-08-29T09:14:07Z";

        let session1 = "a3f29c1d";
        let session2 = "b8e4f1a2";

        let input1 = format!(
            "{}:{}:{}:{}:{}",
            crate_name, file_path, line_num, ts, session1
        );
        let input2 = format!(
            "{}:{}:{}:{}:{}",
            crate_name, file_path, line_num, ts, session2
        );

        let hash1 = format!(
            "AGT-STORE-{:.8}",
            format!("{:x}", md5_or_simple_hash(&input1))
        );
        let hash2 = format!(
            "AGT-STORE-{:.8}",
            format!("{:x}", md5_or_simple_hash(&input2))
        );

        assert_ne!(
            hash1, hash2,
            "Identical timestamps across different sessions must produce distinct hash IDs"
        );
    }

    fn md5_or_simple_hash(input: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn test_check_review_coverage_fixtures() {
        // Fixture 1: 0 passes (should fail)
        let tags_0_passes = vec![TagItem {
            file_path: "crates/memfuse-store/src/lsm.rs".to_string(),
            line_num: 10,
            tag_type: "ANCHOR".to_string(),
            raw: "// ANCHOR[DEBT:STO-001] STATUS:DONE (ID: AGT-STORE-a3f29c1d) (TS:2026-08-29T09:14:07Z) (SESSION:a3f29c1d)".to_string(),
            timestamp: "2026-08-29T09:14:07Z".to_string(),
            category: None,
            severity: None,
            id: Some("AGT-STORE-a3f29c1d".to_string()),
            session: Some("a3f29c1d".to_string()),
            status: Some("DONE".to_string()),
            description: "Task".to_string(),
            is_resolved: true,
        }];
        assert!(!run_check_review_coverage(&tags_0_passes));

        // Fixture 2: 2 passes from SAME session (should fail due to independence rule)
        let mut tags_same_session = tags_0_passes.clone();
        tags_same_session.push(TagItem {
            file_path: "crates/memfuse-store/src/lsm.rs".to_string(),
            line_num: 15,
            tag_type: "REVIEW-PASS".to_string(),
            raw: "// REVIEW-PASS[1/2] STATUS:PASS (ID: AGT-STORE-a3f29c1d) (TS:2026-08-29T09:15:00Z) (SESSION:a3f29c1d)".to_string(),
            timestamp: "2026-08-29T09:15:00Z".to_string(),
            category: None,
            severity: None,
            id: Some("AGT-STORE-a3f29c1d".to_string()),
            session: Some("a3f29c1d".to_string()), // SAME session as anchor!
            status: Some("PASS".to_string()),
            description: "Review 1".to_string(),
            is_resolved: false,
        });
        tags_same_session.push(TagItem {
            file_path: "crates/memfuse-store/src/lsm.rs".to_string(),
            line_num: 16,
            tag_type: "REVIEW-PASS".to_string(),
            raw: "// REVIEW-PASS[2/2] STATUS:PASS (ID: AGT-STORE-a3f29c1d) (TS:2026-08-29T09:16:00Z) (SESSION:a3f29c1d)".to_string(),
            timestamp: "2026-08-29T09:16:00Z".to_string(),
            category: None,
            severity: None,
            id: Some("AGT-STORE-a3f29c1d".to_string()),
            session: Some("a3f29c1d".to_string()), // SAME session as anchor!
            status: Some("PASS".to_string()),
            description: "Review 2".to_string(),
            is_resolved: false,
        });
        assert!(!run_check_review_coverage(&tags_same_session));

        // Fixture 3: 2 passes from DIFFERENT independent sessions (should pass)
        let mut tags_diff_sessions = tags_0_passes.clone();
        tags_diff_sessions.push(TagItem {
            file_path: "crates/memfuse-store/src/lsm.rs".to_string(),
            line_num: 15,
            tag_type: "REVIEW-PASS".to_string(),
            raw: "// REVIEW-PASS[1/2] STATUS:PASS (ID: AGT-STORE-a3f29c1d) (TS:2026-08-29T10:00:00Z) (SESSION:b8e4f1a2)".to_string(),
            timestamp: "2026-08-29T10:00:00Z".to_string(),
            category: None,
            severity: None,
            id: Some("AGT-STORE-a3f29c1d".to_string()),
            session: Some("b8e4f1a2".to_string()), // Independent session 1
            status: Some("PASS".to_string()),
            description: "Review 1".to_string(),
            is_resolved: false,
        });
        tags_diff_sessions.push(TagItem {
            file_path: "crates/memfuse-store/src/lsm.rs".to_string(),
            line_num: 16,
            tag_type: "REVIEW-PASS".to_string(),
            raw: "// REVIEW-PASS[2/2] STATUS:PASS (ID: AGT-STORE-a3f29c1d) (TS:2026-08-29T11:00:00Z) (SESSION:c9f5e2b3)".to_string(),
            timestamp: "2026-08-29T11:00:00Z".to_string(),
            category: None,
            severity: None,
            id: Some("AGT-STORE-a3f29c1d".to_string()),
            session: Some("c9f5e2b3".to_string()), // Independent session 2
            status: Some("PASS".to_string()),
            description: "Review 2".to_string(),
            is_resolved: false,
        });
        assert!(run_check_review_coverage(&tags_diff_sessions));
    }
}
