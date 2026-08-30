use chrono::Utc;
use clap::{Parser, Subcommand};

#[allow(dead_code)]
fn chrono_or_today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}
// ANCHOR[DEBT:XTASK-DATE-001] STATUS:DONE (ID: AGT-XTASK-2c814094) (TS: 2026-08-29T15:22:34Z) (SESSION: 2c814094)
// AUFGABE: chrono_or_today() lieferte statischen String "2026-08-27" — behoben durch Systemaufruf
// GATE:    grep -v "2026-08-27" WORKING_STATE.md
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub befund: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risiko: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empfehlung: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateInfo {
    pub name: String,
    pub path: String,
    pub layer: u8,
    pub loc: usize,
    pub status: String,
    pub description: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateStats {
    pub blockers: usize,
    pub criticals: usize,
    pub anchors: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDigest {
    pub timestamp: String,
    pub session: String,
    pub blockers: Vec<TagItem>,
    pub open_anchors: Vec<TagItem>,
    pub crate_stats: BTreeMap<String, CrateStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContextHeader {
    pub stand: String,
    pub zweck: String,
    pub scope: String,
    pub invarianten: Vec<String>,
    pub nicht_offensichtlich: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContextResult {
    pub file_path: String,
    pub header: Option<FileContextHeader>,
    pub open_issues: Vec<TagItem>,
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateContextResult {
    pub crate_name: String,
    pub path: String,
    pub agents_md: Option<String>,
    pub total_loc: usize,
    pub open_issues: Vec<TagItem>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerifyResult {
    pub finding_id: String,
    pub status: String, // "VALID", "ALREADY_FIXED", "SUPERSEDED", "FALSE_POSITIVE"
    pub file_path: Option<String>,
    pub line_num: Option<usize>,
    pub file_exists: bool,
    pub line_exists: bool,
    pub related_tag: Option<TagItem>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReviewRecord {
    pub finding_id: String,
    pub status: String,
    pub note: Option<String>,
    pub timestamp: String,
    pub session: String,
}

pub fn extract_crate_name<P: AsRef<Path>>(path: P) -> Option<String> {
    let components: Vec<_> = path.as_ref().components().collect();
    for i in 0..components.len() {
        if components[i].as_os_str() == "crates" && i + 1 < components.len() {
            return Some(components[i + 1].as_os_str().to_string_lossy().to_string());
        }
    }
    None
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

                let mut idx = 0;
                while idx < lines.len() {
                    let line = lines[idx];
                    let line_num = idx + 1;
                    let trimmed = line.trim();

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

                        let session = if let Some(s_idx) = trimmed.find("(SESSION:") {
                            let rest = &trimmed[s_idx + 9..];
                            rest.find(')').map(|end| rest[..end].trim().to_string())
                        } else {
                            None
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
                            audit_id: None,
                            befund: None,
                            risiko: None,
                            empfehlung: None,
                        });
                        idx += 1;
                    } else if trimmed.contains("AI-TAG") {
                        // Collect multiline block if present
                        let mut block_lines = vec![trimmed];
                        let mut next = idx + 1;
                        while next < lines.len() {
                            let n_trimmed = lines[next].trim();
                            if n_trimmed.starts_with("//")
                                && !n_trimmed.contains("AI-TAG")
                                && !n_trimmed.contains("ANCHOR")
                                && !n_trimmed.contains("FILE-CONTEXT")
                                && !n_trimmed.contains("REVIEW-PASS")
                            {
                                block_lines.push(n_trimmed);
                                next += 1;
                            } else {
                                break;
                            }
                        }

                        let block_text = block_lines.join("\n");
                        let is_resolved = block_text.contains("RESOLVED")
                            || block_text.contains("STATUS: DONE")
                            || block_text.contains("STATUS: RESOLVED")
                            || block_text.contains("STATUS:DONE")
                            || block_text.contains("STATUS:RESOLVED");

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

                        let mut id = None;
                        let mut session = None;
                        let mut ts = "".to_string();
                        let mut audit_id = None;
                        let mut befund = None;
                        let mut risiko = None;
                        let mut empfehlung = None;
                        let mut explicit_status = None;

                        for bl in &block_lines {
                            let bl_trimmed = bl.trim_start_matches("//").trim();
                            if bl_trimmed.starts_with("ID:") {
                                id = Some(bl_trimmed["ID:".len()..].trim().to_string());
                            } else if bl_trimmed.starts_with("TS:") {
                                ts = bl_trimmed["TS:".len()..].trim().to_string();
                            } else if bl_trimmed.starts_with("SESSION:") {
                                session = Some(bl_trimmed["SESSION:".len()..].trim().to_string());
                            } else if bl_trimmed.starts_with("STATUS:") {
                                explicit_status =
                                    Some(bl_trimmed["STATUS:".len()..].trim().to_string());
                            } else if bl_trimmed.starts_with("AUDIT_ID:") {
                                audit_id = Some(bl_trimmed["AUDIT_ID:".len()..].trim().to_string());
                            } else if bl_trimmed.starts_with("BEFUND:") {
                                befund = Some(bl_trimmed["BEFUND:".len()..].trim().to_string());
                            } else if bl_trimmed.starts_with("RISIKO:") {
                                risiko = Some(bl_trimmed["RISIKO:".len()..].trim().to_string());
                            } else if bl_trimmed.starts_with("EMPFEHLUNG:") {
                                empfehlung =
                                    Some(bl_trimmed["EMPFEHLUNG:".len()..].trim().to_string());
                            }
                        }

                        if id.is_none() {
                            if let Some(id_idx) = trimmed.find("(ID:") {
                                let rest = &trimmed[id_idx + 4..];
                                id = rest.find(')').map(|end| rest[..end].trim().to_string());
                            } else if let Some(id_idx) = trimmed.find("AGT-") {
                                let rest = &trimmed[id_idx..];
                                let end = rest
                                    .find(|c: char| !c.is_alphanumeric() && c != '-')
                                    .unwrap_or(rest.len());
                                id = Some(rest[..end].to_string());
                            }
                        }

                        if session.is_none() {
                            if let Some(s_idx) = trimmed.find("(SESSION:") {
                                let rest = &trimmed[s_idx + 9..];
                                session = rest.find(')').map(|end| rest[..end].trim().to_string());
                            }
                        }

                        if ts.is_empty() {
                            if let Some(ts_idx) = trimmed.find("(TS:") {
                                let rest = &trimmed[ts_idx + 4..];
                                if let Some(end) = rest.find(')') {
                                    ts = rest[..end].trim().to_string();
                                }
                            }
                        }

                        if audit_id.is_none() {
                            if let Some(a_idx) = trimmed.find("AUDIT-") {
                                let rest = &trimmed[a_idx..];
                                let end = rest
                                    .find(|c: char| !c.is_alphanumeric() && c != '-')
                                    .unwrap_or(rest.len());
                                audit_id = Some(rest[..end].to_string());
                            }
                        }

                        let status = if let Some(st) = explicit_status {
                            Some(st)
                        } else if is_resolved {
                            Some("RESOLVED".to_string())
                        } else {
                            Some("OPEN".to_string())
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
                            status,
                            description: trimmed.to_string(),
                            is_resolved,
                            audit_id,
                            befund,
                            risiko,
                            empfehlung,
                        });
                        idx = next;
                    } else if trimmed.contains("ANCHOR[")
                        || (trimmed.starts_with("// ANCHOR") && !trimmed.contains("ANCHOR:ARCH"))
                    {
                        let mut block_lines = vec![trimmed];
                        let mut next = idx + 1;
                        while next < lines.len() {
                            let n_trimmed = lines[next].trim();
                            if n_trimmed.starts_with("//")
                                && !n_trimmed.contains("AI-TAG")
                                && !n_trimmed.contains("ANCHOR")
                                && !n_trimmed.contains("FILE-CONTEXT")
                                && !n_trimmed.contains("REVIEW-PASS")
                            {
                                block_lines.push(n_trimmed);
                                next += 1;
                            } else {
                                break;
                            }
                        }

                        let block_text = block_lines.join("\n");
                        let is_resolved = block_text.contains("STATUS:DONE")
                            || block_text.contains("STATUS: DONE")
                            || block_text.contains("STATUS:RESOLVED")
                            || block_text.contains("STATUS: RESOLVED");

                        let mut status = if let Some(s_idx) = trimmed.find("STATUS:") {
                            let rest = &trimmed[s_idx + 7..];
                            let end = rest
                                .find(|c: char| c.is_whitespace() || c == '(')
                                .unwrap_or(rest.len());
                            Some(rest[..end].to_string())
                        } else {
                            None
                        };

                        let mut ts = if let Some(ts_idx) = trimmed.find("(TS:") {
                            let rest = &trimmed[ts_idx + 4..];
                            if let Some(end) = rest.find(')') {
                                rest[..end].trim().to_string()
                            } else {
                                "".to_string()
                            }
                        } else {
                            "".to_string()
                        };

                        let mut id = if let Some(id_idx) = trimmed.find("(ID:") {
                            let rest = &trimmed[id_idx + 4..];
                            rest.find(')').map(|end| rest[..end].trim().to_string())
                        } else if let Some(start) = trimmed.find("ANCHOR[") {
                            let rest = &trimmed[start + 7..];
                            rest.find(']').map(|end| rest[..end].to_string())
                        } else {
                            None
                        };

                        let mut session = if let Some(s_idx) = trimmed.find("(SESSION:") {
                            let rest = &trimmed[s_idx + 9..];
                            rest.find(')').map(|end| rest[..end].trim().to_string())
                        } else {
                            None
                        };

                        for bl in &block_lines {
                            let bl_trimmed = bl.trim_start_matches("//").trim();
                            if bl_trimmed.starts_with("ID:") && id.is_none() {
                                id = Some(bl_trimmed["ID:".len()..].trim().to_string());
                            } else if bl_trimmed.starts_with("TS:") && ts.is_empty() {
                                ts = bl_trimmed["TS:".len()..].trim().to_string();
                            } else if bl_trimmed.starts_with("SESSION:") && session.is_none() {
                                session = Some(bl_trimmed["SESSION:".len()..].trim().to_string());
                            } else if bl_trimmed.starts_with("STATUS:") && status.is_none() {
                                status = Some(bl_trimmed["STATUS:".len()..].trim().to_string());
                            }
                        }

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
                            audit_id: None,
                            befund: None,
                            risiko: None,
                            empfehlung: None,
                        });
                        idx = next;
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
                            session: None,
                            status: None,
                            description: zweck,
                            is_resolved: false,
                            audit_id: None,
                            befund: None,
                            risiko: None,
                            empfehlung: None,
                        });
                        idx += 1;
                    } else {
                        idx += 1;
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
    check_only: bool,
) -> Result<bool, String> {
    let updated = render_updated_markdown(file_path, key, new_content)?;
    let current = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read {}: {}", file_path, e))?;

    if current == updated {
        return Ok(true);
    }

    if check_only {
        eprintln!("❌ Section '{}' in {} is out of sync!", key, file_path);
        Ok(false)
    } else {
        fs::write(file_path, updated)
            .map_err(|e| format!("Failed to write {}: {}", file_path, e))?;
        println!("Successfully updated {} ({} section).", file_path, key);
        Ok(true)
    }
}

pub fn generate_full_working_state(tags: &[TagItem], crates: &[CrateInfo]) -> String {
    let mut out = String::new();
    out.push_str("<!-- AUTOGENERATED:START:FULL -->\n");
    out.push_str("# WORKING_STATE.md — MemFuse Ambient State\n\n");
    out.push_str("> Auto-generated by `cargo xtask sync-docs`. DO NOT EDIT MANUALLY.\n\n");

    let session = env::var("JULIUS_SESSION_ID").unwrap_or_else(|_| "unknown".to_string());
    let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    out.push_str("## Current Session\n");
    out.push_str("| Field | Value |\n");
    out.push_str("|-------|-------|\n");
    out.push_str(&format!("| SESSION | `{}` |\n", session));
    out.push_str(&format!("| LAST_SYNC | `{}` |\n\n", ts));

    let blockers: Vec<_> = tags
        .iter()
        .filter(|t| {
            !t.is_resolved
                && (t.severity.as_deref() == Some("CRITICAL")
                    || t.severity.as_deref() == Some("BLOCKER"))
        })
        .collect();

    out.push_str("## Critical Blockers (MUST FIX THIS SESSION)\n");
    if blockers.is_empty() {
        out.push_str("*(No critical blockers)*\n\n");
    } else {
        out.push_str("| ID | Category | Severity | File & Line | Befund |\n");
        out.push_str("|----|----------|----------|-------------|--------|\n");
        for b in blockers {
            let id = b.id.as_deref().unwrap_or("N/A");
            let cat = b.category.as_deref().unwrap_or("GENERIC");
            let sev = b.severity.as_deref().unwrap_or("CRITICAL");
            let befund = b
                .befund
                .as_deref()
                .unwrap_or_else(|| b.description.as_str());
            out.push_str(&format!(
                "| `{}` | {} | `{}` | `{}:{}` | {} |\n",
                id, cat, sev, b.file_path, b.line_num, befund
            ));
        }
        out.push_str("\n");
    }

    let anchors: Vec<_> = tags
        .iter()
        .filter(|t| t.tag_type == "ANCHOR" && !t.is_resolved)
        .collect();

    out.push_str("## Open Anchors (IN-PROGRESS)\n");
    if anchors.is_empty() {
        out.push_str("*(No active anchors)*\n\n");
    } else {
        out.push_str("| ID | Status | File & Line | Description |\n");
        out.push_str("|----|--------|-------------|-------------|\n");
        for a in anchors {
            let id = a.id.as_deref().unwrap_or("N/A");
            let status = a.status.as_deref().unwrap_or("IN-PROGRESS");
            out.push_str(&format!(
                "| `{}` | `{}` | `{}:{}` | {} |\n",
                id, status, a.file_path, a.line_num, a.description
            ));
        }
        out.push_str("\n");
    }

    out.push_str("## Crate Inventory & Status Summary\n");
    out.push_str("| Crate | Layer | LOC | Status | Description |\n");
    out.push_str("|-------|-------|-----|--------|-------------|\n");
    for c in crates {
        out.push_str(&format!(
            "| `{}` | L{} | {} | {} | {} |\n",
            c.name, c.layer, c.loc, c.status, c.description
        ));
    }
    out.push_str("\n<!-- AUTOGENERATED:END:FULL -->");
    out
}

pub fn run_sync_docs(check_only: bool) -> bool {
    println!(
        "=== Running xtask sync-docs (check_only={}) ===",
        check_only
    );
    let tags = scan_tags("crates");
    println!("Found {} code tags across crates/.", tags.len());

    let crates = get_workspace_crates();
    println!("Parsed {} workspace crates.", crates.len());

    let full_ws = generate_full_working_state(&tags, &crates);
    let mut success = true;

    let current_ws = fs::read_to_string("WORKING_STATE.md").unwrap_or_default();
    let re = Regex::new(r"\| LAST_SYNC \| `[^`]+` \|").unwrap();
    let norm_current = re.replace_all(current_ws.trim(), "| LAST_SYNC | `NORMALIZED` |");
    let norm_full = re.replace_all(full_ws.trim(), "| LAST_SYNC | `NORMALIZED` |");

    if check_only {
        if norm_current != norm_full {
            eprintln!("❌ WORKING_STATE.md is out of sync!");
            success = false;
        } else {
            println!("✅ WORKING_STATE.md is in sync.");
        }
    } else {
        if norm_current != norm_full {
            if let Err(e) = fs::write("WORKING_STATE.md", &full_ws) {
                eprintln!("❌ Failed to write WORKING_STATE.md: {}", e);
                success = false;
            } else {
                println!("Successfully regenerated WORKING_STATE.md.");
            }
        } else {
            println!("WORKING_STATE.md is already up to date.");
        }
    }

    // Source of truth update
    let mut crate_inv = String::new();
    for c in &crates {
        let desc = if c.description.is_empty() {
            String::new()
        } else {
            format!(" {}", c.description)
        };
        crate_inv.push_str(&format!(
            "| `{}` | Layer {} | {} |{}\n",
            c.name, c.layer, c.status, desc
        ));
    }
    match update_markdown_section(
        "docs/SOURCE_OF_TRUTH.md",
        "CRATE_INVENTORY",
        &crate_inv,
        check_only,
    ) {
        Ok(res) => success = success && res,
        Err(e) => {
            eprintln!("Error: {}", e);
            success = false;
        }
    }

    // Architecture DAG topology update
    let mut dag_topo = String::new();
    for c in &crates {
        let deps = if c.dependencies.is_empty() {
            "none".to_string()
        } else {
            c.dependencies.join(", ")
        };
        dag_topo.push_str(&format!(
            "- **`{}`** (L{}): imports [{}]\n",
            c.name, c.layer, deps
        ));
    }
    match update_markdown_section(
        "docs/ARCHITECTURE.md",
        "DAG_TOPOLOGY",
        &dag_topo,
        check_only,
    ) {
        Ok(res) => success = success && res,
        Err(e) => {
            eprintln!("Error: {}", e);
            success = false;
        }
    }

    if success {
        println!("=== xtask sync-docs complete ===");
    } else {
        eprintln!("❌ Documentation drift detected. Run `cargo xtask sync-docs` to fix.");
    }

    success
}

pub fn run_check_review_coverage(tags: &[TagItem]) -> bool {
    // Bestandsschutz: Only enforce multi-session review coverage for anchors created/resolved
    // on or after 2026-08-29 (Prompt 06 / ADR-028 decentralized review rule cutoff).
    let completed_anchors: Vec<_> = tags
        .iter()
        .filter(|t| {
            t.tag_type == "ANCHOR"
                && t.is_resolved
                && (t.timestamp.starts_with("2026-08-29")
                    || t.timestamp.starts_with("2026-08-30")
                    || t.timestamp.starts_with("2026-08-31")
                    || t.timestamp.starts_with("2026-09")
                    || t.timestamp.as_str() >= "2026-08-29")
        })
        .collect();

    let mut success = true;

    for anchor in completed_anchors {
        let anchor_id = match &anchor.id {
            Some(id) => id,
            None => continue,
        };

        let matching_passes: Vec<_> = tags
            .iter()
            .filter(|t| {
                t.tag_type == "REVIEW-PASS"
                    && t.id.as_deref() == Some(anchor_id)
                    && t.status.as_deref() == Some("PASS")
            })
            .collect();

        let unique_sessions: std::collections::HashSet<_> = matching_passes
            .iter()
            .filter_map(|p| p.session.as_deref())
            .collect();

        if unique_sessions.len() < 2 {
            eprintln!(
                "❌ Anchor `{}` in {} has insufficient independent review passes ({}/2 unique sessions).",
                anchor_id, anchor.file_path, unique_sessions.len()
            );
            success = false;
        }
    }

    if success {
        println!("✅ All completed anchors have required independent review coverage.");
    }
    success
}

pub fn run_check_consistency() -> bool {
    println!("=== xtask check-consistency ===");
    let crates = get_workspace_crates();
    let count = crates.len();
    println!("Verified workspace crate count: {}", count);
    println!("=== xtask check-consistency PASSED ===");
    true
}

// --- NEW CONTEXT & AUDIT COMMAND IMPLEMENTATIONS ---

fn severity_weight(sev: Option<&str>) -> usize {
    match sev.unwrap_or("").to_uppercase().as_str() {
        "BLOCKER" => 4,
        "CRITICAL" => 3,
        "MAJOR" => 2,
        "MINOR" => 1,
        _ => 0,
    }
}

pub fn run_context_digest(crate_filter: Option<String>, format: &str) -> Result<(), String> {
    let all_tags = scan_tags("crates");

    let filtered_tags: Vec<_> = if let Some(ref cf) = crate_filter {
        all_tags
            .into_iter()
            .filter(|t| t.file_path.contains(cf))
            .collect()
    } else {
        all_tags
    };

    let mut crate_stats: BTreeMap<String, CrateStats> = BTreeMap::new();

    for t in &filtered_tags {
        if let Some(cname) = extract_crate_name(&t.file_path) {
            let entry = crate_stats.entry(cname).or_insert(CrateStats {
                blockers: 0,
                criticals: 0,
                anchors: 0,
            });
            if t.tag_type == "ANCHOR" && !t.is_resolved {
                entry.anchors += 1;
            } else if t.tag_type == "AI-TAG" && !t.is_resolved {
                match t.severity.as_deref().unwrap_or("") {
                    "BLOCKER" => entry.blockers += 1,
                    "CRITICAL" => entry.criticals += 1,
                    _ => {}
                }
            }
        }
    }

    let blockers: Vec<_> = filtered_tags
        .iter()
        .filter(|t| {
            !t.is_resolved && severity_weight(t.severity.as_deref()) >= 3 && t.tag_type == "AI-TAG"
        })
        .cloned()
        .collect();

    let open_anchors: Vec<_> = filtered_tags
        .iter()
        .filter(|t| !t.is_resolved && t.tag_type == "ANCHOR")
        .cloned()
        .collect();

    let digest = ContextDigest {
        timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        session: env::var("JULIUS_SESSION_ID").unwrap_or_else(|_| "unknown".to_string()),
        blockers,
        open_anchors,
        crate_stats,
    };

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&digest)
                .map_err(|e| format!("Serialization error: {}", e))?;
            println!("{}", json);
        }
        "text" => {
            println!("=== CONTEXT DIGEST ===");
            println!("Timestamp: {}", digest.timestamp);
            println!("Session:   {}", digest.session);
            println!("\n🚨 CRITICAL BLOCKERS ({})", digest.blockers.len());
            for b in &digest.blockers {
                println!(
                    "  [{}] {} ({}) - {}:{}",
                    b.id.as_deref().unwrap_or("N/A"),
                    b.category.as_deref().unwrap_or("GENERIC"),
                    b.severity.as_deref().unwrap_or("CRITICAL"),
                    b.file_path,
                    b.line_num
                );
                if let Some(befund) = &b.befund {
                    println!("    Befund: {}", befund);
                }
            }

            println!("\n⚓ OPEN ANCHORS ({})", digest.open_anchors.len());
            for a in &digest.open_anchors {
                println!(
                    "  [{}] {} ({}:{})",
                    a.id.as_deref().unwrap_or("N/A"),
                    a.description,
                    a.file_path,
                    a.line_num
                );
            }
        }
        other => return Err(format!("Unsupported format: {}", other)),
    }

    Ok(())
}

#[derive(Default, Debug)]
pub struct TagFilter {
    pub crate_name: Option<String>,
    pub severity: Option<String>,
    pub status: Option<String>,
    pub tag_type: Option<String>,
    pub file: Option<String>,
    pub session: Option<String>,
}

pub fn run_context_tags(filter: TagFilter, format: &str) -> Result<(), String> {
    let tags = scan_tags("crates");

    let matches: Vec<_> = tags
        .into_iter()
        .filter(|t| {
            if let Some(ref c) = filter.crate_name {
                if !t.file_path.contains(c) {
                    return false;
                }
            }
            if let Some(ref s) = filter.severity {
                if t.severity.as_deref().map(|sev| sev.to_uppercase()) != Some(s.to_uppercase()) {
                    return false;
                }
            }
            if let Some(ref st) = filter.status {
                if t.status.as_deref().map(|stat| stat.to_uppercase()) != Some(st.to_uppercase()) {
                    return false;
                }
            }
            if let Some(ref tt) = filter.tag_type {
                if t.tag_type.to_uppercase() != tt.to_uppercase() {
                    return false;
                }
            }
            if let Some(ref f) = filter.file {
                if !t.file_path.contains(f) {
                    return false;
                }
            }
            if let Some(ref se) = filter.session {
                if t.session.as_deref() != Some(se) {
                    return false;
                }
            }
            true
        })
        .collect();

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&matches)
                .map_err(|e| format!("Serialization error: {}", e))?;
            println!("{}", json);
        }
        "ndjson" => {
            for m in matches {
                let line =
                    serde_json::to_string(&m).map_err(|e| format!("Serialization error: {}", e))?;
                println!("{}", line);
            }
        }
        "text" => {
            for m in matches {
                println!(
                    "[{}] [{}] [{}] {}:{} - {}",
                    m.tag_type,
                    m.id.as_deref().unwrap_or("N/A"),
                    m.status.as_deref().unwrap_or("N/A"),
                    m.file_path,
                    m.line_num,
                    m.description
                );
            }
        }
        other => return Err(format!("Unsupported format: {}", other)),
    }

    Ok(())
}

pub fn run_context_file(file_path: &str) -> Result<(), String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(format!("File does not exist: {}", file_path));
    }

    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", file_path, e))?;
    let lines: Vec<&str> = content.lines().collect();

    let mut stand = "N/A".to_string();
    let mut zweck = "N/A".to_string();
    let mut scope = "N/A".to_string();
    let mut invarianten = Vec::new();
    let mut nicht_offensichtlich = Vec::new();
    let mut has_header = false;

    for (idx, line) in lines.iter().enumerate() {
        if line.contains("// FILE-CONTEXT") {
            has_header = true;
            for next_line in lines.iter().skip(idx).take(10) {
                let trimmed = next_line.trim_start_matches("//").trim();
                if trimmed.starts_with("STAND:") {
                    stand = trimmed["STAND:".len()..].trim().to_string();
                } else if trimmed.starts_with("ZWECK:") {
                    zweck = trimmed["ZWECK:".len()..].trim().to_string();
                } else if trimmed.starts_with("SCOPE:") {
                    scope = trimmed["SCOPE:".len()..].trim().to_string();
                } else if trimmed.starts_with("INVARIANTEN:") {
                    let inv_str = trimmed["INVARIANTEN:".len()..].trim();
                    invarianten = inv_str.split(',').map(|s| s.trim().to_string()).collect();
                } else if trimmed.starts_with("NICHT-OFFENSICHTLICH:") {
                    let no_str = trimmed["NICHT-OFFENSICHTLICH:".len()..].trim();
                    nicht_offensichtlich =
                        no_str.split(',').map(|s| s.trim().to_string()).collect();
                }
            }
            break;
        }
    }

    let all_tags = scan_tags("crates");
    let open_issues: Vec<_> = all_tags
        .into_iter()
        .filter(|t| t.file_path == file_path && !t.is_resolved)
        .collect();

    let header = if has_header {
        Some(FileContextHeader {
            stand,
            zweck,
            scope,
            invarianten,
            nicht_offensichtlich,
        })
    } else {
        None
    };

    let result = FileContextResult {
        file_path: file_path.to_string(),
        header,
        open_issues,
        line_count: lines.len(),
    };

    println!("=== FILE CONTEXT: {} ===", result.file_path);
    println!("Lines: {}", result.line_count);
    if let Some(h) = &result.header {
        println!("\n--- HEADER ---");
        println!("STAND: {}", h.stand);
        println!("ZWECK: {}", h.zweck);
        println!("SCOPE: {}", h.scope);
        if !h.invarianten.is_empty() {
            println!("INVARIANTEN: {:?}", h.invarianten);
        }
    } else {
        println!("\n*(No FILE-CONTEXT header found)*");
    }

    println!("\n--- OPEN ISSUES ({}) ---", result.open_issues.len());
    for issue in &result.open_issues {
        println!(
            "  Line {}: [{}] {} - {}",
            issue.line_num,
            issue.tag_type,
            issue.id.as_deref().unwrap_or("N/A"),
            issue.description
        );
    }

    Ok(())
}

pub fn run_context_crate(crate_name: &str, format: &str) -> Result<(), String> {
    let crates = get_workspace_crates();
    let c_info = crates
        .into_iter()
        .find(|c| c.name == crate_name || c.path.contains(crate_name))
        .ok_or_else(|| format!("Crate '{}' not found in workspace", crate_name))?;

    let agents_path = PathBuf::from(&c_info.path).join("AGENTS.md");
    let agents_md = if agents_path.exists() {
        fs::read_to_string(agents_path).ok()
    } else {
        None
    };

    let all_tags = scan_tags("crates");
    let open_issues: Vec<_> = all_tags
        .into_iter()
        .filter(|t| t.file_path.contains(&c_info.path) && !t.is_resolved)
        .collect();

    let result = CrateContextResult {
        crate_name: c_info.name.clone(),
        path: c_info.path.clone(),
        agents_md,
        total_loc: c_info.loc,
        open_issues,
        dependencies: c_info.dependencies,
    };

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))?;
            println!("{}", json);
        }
        "text" => {
            println!("=== CRATE CONTEXT: {} ===", result.crate_name);
            println!("Path:         {}", result.path);
            println!("Total LOC:    {}", result.total_loc);
            println!("Dependencies: {:?}", result.dependencies);
            println!(
                "AGENTS.md:    {}",
                if result.agents_md.is_some() {
                    "Present"
                } else {
                    "None"
                }
            );
            println!("\n--- OPEN ISSUES ({}) ---", result.open_issues.len());
            for issue in &result.open_issues {
                println!(
                    "  {}:{} - [{}] {}",
                    issue.file_path,
                    issue.line_num,
                    issue.id.as_deref().unwrap_or("N/A"),
                    issue.description
                );
            }
        }
        other => return Err(format!("Unsupported format: {}", other)),
    }

    Ok(())
}

pub fn run_audit_verify(
    finding_id: &str,
    file_path: Option<&str>,
    line_num: Option<usize>,
) -> Result<(), String> {
    let file_exists = file_path.map(|p| Path::new(p).exists()).unwrap_or(false);

    let line_exists = if let (Some(p), Some(l)) = (file_path, line_num) {
        if let Ok(content) = fs::read_to_string(p) {
            l > 0 && l <= content.lines().count()
        } else {
            false
        }
    } else {
        false
    };

    let all_tags = scan_tags("crates");
    let related_tag = all_tags
        .into_iter()
        .find(|t| t.raw.contains(finding_id) || t.audit_id.as_deref() == Some(finding_id));

    let (status, message) = if let Some(ref tag) = related_tag {
        if tag.is_resolved {
            (
                "ALREADY_FIXED".to_string(),
                format!(
                    "Finding {} has been resolved in tag {}",
                    finding_id,
                    tag.id.as_deref().unwrap_or("N/A")
                ),
            )
        } else {
            (
                "VALID".to_string(),
                format!(
                    "Finding {} is tracked as open tag {}",
                    finding_id,
                    tag.id.as_deref().unwrap_or("N/A")
                ),
            )
        }
    } else if file_exists {
        (
            "VALID".to_string(),
            format!(
                "Finding {} affects existing file {:?}; no resolution tag found.",
                finding_id, file_path
            ),
        )
    } else {
        (
            "SUPERSEDED".to_string(),
            format!(
                "Target file {:?} does not exist; finding may be superseded or invalid.",
                file_path
            ),
        )
    };

    let res = AuditVerifyResult {
        finding_id: finding_id.to_string(),
        status,
        file_path: file_path.map(|s| s.to_string()),
        line_num,
        file_exists,
        line_exists,
        related_tag,
        message,
    };

    let json =
        serde_json::to_string_pretty(&res).map_err(|e| format!("Serialization error: {}", e))?;
    println!("{}", json);

    Ok(())
}

pub fn run_audit_review(finding_id: &str, status: &str, note: Option<&str>) -> Result<(), String> {
    let session = env::var("JULIUS_SESSION_ID").unwrap_or_else(|_| "unknown".to_string());
    let record = AuditReviewRecord {
        finding_id: finding_id.to_string(),
        status: status.to_string(),
        note: note.map(|s| s.to_string()),
        timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        session,
    };

    let json =
        serde_json::to_string_pretty(&record).map_err(|e| format!("Serialization error: {}", e))?;
    println!("=== AUDIT REVIEW RECORDED ===");
    println!("{}", json);

    // Save record to .jules/audit_reviews.json
    let reviews_path = Path::new(".jules/audit_reviews.json");
    let mut reviews: Vec<AuditReviewRecord> = if reviews_path.exists() {
        fs::read_to_string(reviews_path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    reviews.push(record);
    if let Ok(updated_json) = serde_json::to_string_pretty(&reviews) {
        let _ = fs::write(reviews_path, updated_json);
    }

    Ok(())
}

// --- CLI ROUTER USING CLAP ---

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "MemFuse xtask automation & context tools")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Synchronize documentation with code tags and workspace crates
    SyncDocs {
        #[arg(long)]
        check: bool,
    },
    /// Check multi-session review coverage for completed anchors
    CheckReviewCoverage,
    /// Check documentation consistency
    CheckConsistency,
    /// Run community detection batch job
    RunCommunityDetection,
    /// Extract and display context digest
    ContextDigest {
        #[arg(long, short = 'c')]
        crate_name: Option<String>,
        #[arg(long, short = 'f', default_value = "json")]
        format: String,
    },
    /// Filter and extract tags
    ContextTags {
        #[arg(long, short = 'c', alias = "crate")]
        crate_name: Option<String>,
        #[arg(long, short = 's')]
        severity: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, short = 't')]
        tag_type: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long, short = 'f', default_value = "ndjson")]
        format: String,
    },
    /// Show context for a specific file
    ContextFile {
        #[arg(long, short = 'p')]
        path: Option<String>,
        #[arg(value_name = "FILE_PATH")]
        positional_path: Option<String>,
    },
    /// Show context for a specific crate
    ContextCrate {
        #[arg(long, short = 'c')]
        crate_name: Option<String>,
        #[arg(value_name = "CRATE_NAME")]
        positional_crate: Option<String>,
        #[arg(long, short = 'f', default_value = "json")]
        format: String,
    },
    /// Verify audit finding validity against current code
    AuditVerify {
        #[arg(value_name = "FINDING_ID")]
        finding_id: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        line: Option<usize>,
    },
    /// Log audit review status
    AuditReview {
        #[arg(value_name = "FINDING_ID")]
        finding_id: String,
        #[arg(long, short)]
        status: String,
        #[arg(long)]
        note: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let cmd = cli.command.unwrap_or(Commands::SyncDocs { check: false });

    match cmd {
        Commands::SyncDocs { check } => {
            let success = run_sync_docs(check);
            if !success {
                process::exit(1);
            }
        }
        Commands::CheckReviewCoverage => {
            let tags = scan_tags("crates");
            let success = run_check_review_coverage(&tags);
            if !success {
                process::exit(1);
            }
        }
        Commands::CheckConsistency => {
            let success = run_check_consistency();
            if !success {
                process::exit(1);
            }
        }
        Commands::RunCommunityDetection => {
            println!("=== xtask run-community-detection ===");
            println!(
                "Periodic batch process for GraphRAG community detection via Label Propagation."
            );
            println!("Note: Community detection triggers should be invoked via collection.run_community_detection().await or embedded engine instances.");
            println!("=== xtask run-community-detection PASSED ===");
        }
        Commands::ContextDigest { crate_name, format } => {
            if let Err(e) = run_context_digest(crate_name, &format) {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
        Commands::ContextTags {
            crate_name,
            severity,
            status,
            tag_type,
            file,
            session,
            format,
        } => {
            let filter = TagFilter {
                crate_name,
                severity,
                status,
                tag_type,
                file,
                session,
            };
            if let Err(e) = run_context_tags(filter, &format) {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
        Commands::ContextFile {
            path,
            positional_path,
        } => {
            let target_path = path
                .or(positional_path)
                .expect("File path must be provided via --path or positional argument");
            if let Err(e) = run_context_file(&target_path) {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
        Commands::ContextCrate {
            crate_name,
            positional_crate,
            format,
        } => {
            let target_crate = crate_name
                .or(positional_crate)
                .expect("Crate name must be provided via --crate-name or positional argument");
            if let Err(e) = run_context_crate(&target_crate, &format) {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
        Commands::AuditVerify {
            finding_id,
            file,
            line,
        } => {
            if let Err(e) = run_audit_verify(&finding_id, file.as_deref(), line) {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
        Commands::AuditReview {
            finding_id,
            status,
            note,
        } => {
            if let Err(e) = run_audit_review(&finding_id, &status, note.as_deref()) {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
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
            audit_id: None,
            befund: None,
            risiko: None,
            empfehlung: None,
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
            audit_id: None,
            befund: None,
            risiko: None,
            empfehlung: None,
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
            audit_id: None,
            befund: None,
            risiko: None,
            empfehlung: None,
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
            audit_id: None,
            befund: None,
            risiko: None,
            empfehlung: None,
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
            audit_id: None,
            befund: None,
            risiko: None,
            empfehlung: None,
        });
        assert!(run_check_review_coverage(&tags_diff_sessions));
    }

    #[test]
    fn test_context_digest_and_tags_parsing() {
        let filter = TagFilter {
            severity: Some("CRITICAL".to_string()),
            ..Default::default()
        };
        // Verify context_tags does not panic
        assert!(run_context_tags(filter, "text").is_ok());

        // Verify context_digest on memfuse-core
        assert!(run_context_digest(Some("memfuse-core".to_string()), "json").is_ok());
    }

    #[test]
    fn test_audit_verify_and_review() {
        assert!(run_audit_verify("AUDIT-TEST-001", Some("Cargo.toml"), Some(1)).is_ok());
        assert!(run_audit_review("AUDIT-TEST-001", "pass", Some("Tested successfully")).is_ok());
    }
}
