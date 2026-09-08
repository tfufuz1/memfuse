// MemFuse — Check Placeholder References & Unresolved ADR References Gate
//
// Modul zur Überprüfung von Platzhalter-Referenzen (z. B. ADR-0XX, TBD, TODO-ADR, <...>)
// und nicht existierenden ADR-Dateireferenzen in Governance-Dokumenten (VETOES.md, AGENTS.md, docs/decisions/*.md).

use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::find_root_dir;

#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
pub enum ViolationReason {
    Placeholder,
    MissingFile,
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
pub struct PlaceholderViolation {
    pub file: String,
    pub line: usize,
    pub matched_text: String,
    pub reason: ViolationReason,
}

pub fn collect_target_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    let vetoes = root.join("VETOES.md");
    if vetoes.is_file() {
        files.push(vetoes);
    }

    let agents = root.join("AGENTS.md");
    if agents.is_file() {
        files.push(agents);
    }

    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        for entry in walkdir::WalkDir::new(&crates_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() && entry.file_name() == "AGENTS.md" {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    let decisions_dir = root.join("docs").join("decisions");
    if decisions_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&decisions_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    files.dedup();
    files
}

pub fn check_placeholder_refs_in_content(
    content: &str,
    file_rel_path: &str,
    root: &Path,
) -> Vec<PlaceholderViolation> {
    let mut violations = Vec::new();

    // Regex für Platzhalter-Muster nach Ankern: ADR-0?X+\b, \bTBD\b, \bTODO-ADR\b, <[^>]+>
    let placeholder_re =
        Regex::new(r"(ADR-0?X+\b|\bTBD\b|\bTODO-ADR\b|<[^>]+>)").expect("Valid placeholder regex");

    let anchors = ["adr_ref:", "DECISION-REF:"];

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;

        for anchor in &anchors {
            if let Some(anchor_pos) = line.find(anchor) {
                let after_anchor = &line[anchor_pos + anchor.len()..];
                let val_str = after_anchor.trim();
                let cleaned_val = val_str.trim_matches(|c: char| {
                    c == '"' || c == '\'' || c == ')' || c == '(' || c == '`'
                });

                if placeholder_re.is_match(val_str) {
                    violations.push(PlaceholderViolation {
                        file: file_rel_path.to_string(),
                        line: line_num,
                        matched_text: val_str.to_string(),
                        reason: ViolationReason::Placeholder,
                    });
                } else if *anchor == "adr_ref:" {
                    let first_word = cleaned_val.split_whitespace().next().unwrap_or("");
                    let is_null = first_word.is_empty()
                        || first_word.eq_ignore_ascii_case("null")
                        || first_word.eq_ignore_ascii_case("none")
                        || first_word == "~";

                    if !is_null {
                        let file_exists =
                            root.join(first_word).exists() || Path::new(first_word).exists();
                        let found_in_decisions = if !file_exists {
                            let decisions_file = root.join("DECISIONS.md");
                            if decisions_file.is_file() {
                                if let Ok(dec_content) = fs::read_to_string(&decisions_file) {
                                    let adr_re = Regex::new(r"ADR-(\d+)").unwrap();
                                    if let Some(caps) = adr_re.captures(first_word) {
                                        let pattern = format!("ADR-{}", &caps[1]);
                                        dec_content.contains(&pattern)
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        if !file_exists && !found_in_decisions {
                            violations.push(PlaceholderViolation {
                                file: file_rel_path.to_string(),
                                line: line_num,
                                matched_text: first_word.to_string(),
                                reason: ViolationReason::MissingFile,
                            });
                        }
                    }
                }
            }
        }
    }

    violations
}

pub fn check_placeholder_refs(root: &Path) -> Vec<PlaceholderViolation> {
    let mut violations = Vec::new();
    let target_files = collect_target_files(root);

    for file_path in target_files {
        let rel_path = file_path
            .strip_prefix(root)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();

        if let Ok(content) = fs::read_to_string(&file_path) {
            let file_violations = check_placeholder_refs_in_content(&content, &rel_path, root);
            violations.extend(file_violations);
        }
    }

    violations
}

pub fn run() -> Result<(), String> {
    println!("=== Running xtask check-placeholder-refs ===");
    let root = find_root_dir();
    let violations = check_placeholder_refs(&root);

    if !violations.is_empty() {
        for v in &violations {
            let reason_str = match v.reason {
                ViolationReason::Placeholder => "Offener Platzhalter in Referenzfeld",
                ViolationReason::MissingFile => "Referenzierte Datei existiert nicht",
            };
            eprintln!(
                "❌ [GATE-13]: {}:{} — {}: '{}'",
                v.file, v.line, reason_str, v.matched_text
            );
        }
        return Err(format!(
            "check-placeholder-refs failed with {} violation(s)",
            violations.len()
        ));
    }

    println!("✅ Keine offenen Platzhalter-Referenzen oder fehlenden Dateien in Governance-Dokumenten gefunden.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positive_missing_file() {
        let root = crate::find_root_dir();
        let content = "adr_ref: docs/decisions/ADR-999-nonexistent.md";
        let violations = check_placeholder_refs_in_content(content, "test.md", &root);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].reason, ViolationReason::MissingFile);
        assert_eq!(violations[0].file, "test.md");
        assert_eq!(violations[0].line, 1);
        assert_eq!(
            violations[0].matched_text,
            "docs/decisions/ADR-999-nonexistent.md"
        );
    }

    #[test]
    fn test_positive_placeholder() {
        let root = crate::find_root_dir();
        let content = "adr_ref: docs/decisions/ADR-0XX-platzhalter.md";
        let violations = check_placeholder_refs_in_content(content, "test.md", &root);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].reason, ViolationReason::Placeholder);
        assert_eq!(violations[0].file, "test.md");
        assert_eq!(violations[0].line, 1);
    }

    #[test]
    fn test_negative_null() {
        let root = crate::find_root_dir();
        let content = "adr_ref: null";
        let violations = check_placeholder_refs_in_content(content, "test.md", &root);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_negative_existing_adr() {
        let root = crate::find_root_dir();
        let content = "adr_ref: docs/decisions/ADR-001-lsm-tree-für-persistenz.md";
        let violations = check_placeholder_refs_in_content(content, "test.md", &root);
        assert!(violations.is_empty());
    }
}
