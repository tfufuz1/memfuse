// MemFuse — Check Documentation Path References Gate
//
// Subkommando `cargo xtask check-doc-references`
// Durchsucht Markdown-Dateien nach Datei-/Modul-Pfad-Referenzen und prüft,
// ob die referenzierten Pfade im Repository existieren.

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const DEFAULT_CHECK_DOCS: &[&str] = &[
    "docs/GESAMTSPEZIFIKATION_v10.md",
    "AGENTS.md",
    "DECISIONS.md",
];

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DocRefViolation {
    pub file: String,
    pub line: usize,
    pub referenced_path: String,
}

pub fn extract_path_candidates(line: &str) -> Vec<String> {
    if line.contains("<!-- doc-ref-ignore -->") {
        return Vec::new();
    }

    let mut candidates = Vec::new();

    // Matching inline code blocks `...` and word tokens ending with .rs, .toml, or .md
    let re = Regex::new(r"`([^`]+)`|\b([a-zA-Z0-9_\-\.\/\*]+\.(?:rs|toml|md))\b").unwrap();

    for cap in re.captures_iter(line) {
        let token_raw = cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str());
        if let Some(raw) = token_raw {
            let trimmed = raw.trim_matches(|c: char| {
                c == '`'
                    || c == '\''
                    || c == '"'
                    || c == '('
                    || c == ')'
                    || c == '['
                    || c == ']'
                    || c == '{'
                    || c == '}'
                    || c == '<'
                    || c == '>'
                    || c == ','
                    || c == ':'
                    || c == ';'
                    || c == '|'
                    || c == '?'
                    || c == '!'
                    || c == '~'
                    || c == ' '
            });

            if trimmed.ends_with(".rs") || trimmed.ends_with(".toml") || trimmed.ends_with(".md") {
                // Filter out generic prose/extension strings
                if trimmed == ".rs"
                    || trimmed == ".toml"
                    || trimmed == ".md"
                    || trimmed == "*.rs"
                    || trimmed == "*.toml"
                    || trimmed == "*.md"
                {
                    continue;
                }

                if trimmed.contains(' ') {
                    continue;
                }

                // Exclude leading wildcard or lone dot unless valid path prefix
                if (trimmed.starts_with('*') && !trimmed.contains('/'))
                    || (trimmed.starts_with('.')
                        && !trimmed.starts_with("./")
                        && !trimmed.starts_with("../")
                        && !trimmed.starts_with(".jules/"))
                {
                    continue;
                }

                let stem = if let Some(idx) = trimmed.rfind('.') {
                    &trimmed[..idx]
                } else {
                    ""
                };

                if stem.is_empty() || stem == "." || stem == ".." {
                    continue;
                }

                candidates.push(trimmed.to_string());
            }
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

pub fn resolve_path_reference(p: &str, root: &Path) -> bool {
    let p_clean = p.trim_start_matches("./");

    // 1. Direct path relative to root
    let direct = root.join(p_clean);
    if direct.exists() {
        return true;
    }

    // 2. Path relative to crates/
    let in_crates = root.join("crates").join(p_clean);
    if in_crates.exists() {
        return true;
    }

    // 3. Path relative to benchmarks/
    let in_benchmarks = root.join("benchmarks").join(p_clean);
    if in_benchmarks.exists() {
        return true;
    }

    // 4. Wildcard path matching (e.g., docs/decisions/*.md)
    if p_clean.contains('*') {
        if let Some(star_pos) = p_clean.find('*') {
            let dir_part = &p_clean[..star_pos];
            let suffix_part = &p_clean[star_pos + 1..];

            for search_base in &[root.to_path_buf(), root.join("crates")] {
                let check_dir = search_base.join(dir_part.trim_end_matches('/'));
                if check_dir.is_dir() {
                    if let Ok(read_dir) = fs::read_dir(&check_dir) {
                        for entry in read_dir.filter_map(|e| e.ok()) {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.ends_with(suffix_part) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    // 5. Filename-only search when no directory separator '/' is present
    if !p_clean.contains('/') {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                name != "target" && name != ".git" && name != "node_modules"
            })
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() && entry.file_name().to_string_lossy() == p_clean {
                return true;
            }
        }
    }

    false
}

pub fn check_doc_references_in_content(
    content: &str,
    doc_rel_path: &str,
    root: &Path,
) -> Vec<DocRefViolation> {
    let mut violations = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let candidates = extract_path_candidates(line);

        for cand in candidates {
            if !resolve_path_reference(&cand, root) {
                violations.push(DocRefViolation {
                    file: doc_rel_path.to_string(),
                    line: line_num,
                    referenced_path: cand,
                });
            }
        }
    }

    violations
}

pub fn check_doc_references_for_files(
    doc_files: &[impl AsRef<Path>],
    root: &Path,
) -> Vec<DocRefViolation> {
    let mut violations = Vec::new();

    for doc_file in doc_files {
        let path = doc_file.as_ref();
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };

        let rel_path = path.to_string_lossy().to_string();

        if full_path.is_file() {
            if let Ok(content) = fs::read_to_string(&full_path) {
                let file_violations = check_doc_references_in_content(&content, &rel_path, root);
                violations.extend(file_violations);
            }
        }
    }

    violations
}

pub fn run_check_doc_references() -> Result<(), String> {
    println!("=== Running xtask check-doc-references ===");
    let root = crate::find_root_dir();
    let doc_paths: Vec<PathBuf> = DEFAULT_CHECK_DOCS.iter().map(PathBuf::from).collect();

    let violations = check_doc_references_for_files(&doc_paths, &root);

    if !violations.is_empty() {
        for v in &violations {
            eprintln!(
                "❌ [check-doc-references]: {}:{} — verwaiste Pfad-Referenz: '{}'",
                v.file, v.line, v.referenced_path
            );
        }
        return Err(format!(
            "check-doc-references failed with {} stale reference(s)",
            violations.len()
        ));
    }

    println!("✅ Alle Pfad-Referenzen in Governance-Dokumenten existieren im Repository.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_candidates() {
        let line = "Referenz auf `crates/memfuse-core/src/lib.rs` und `physio_scheduler.rs` sowie `Cargo.toml`. Ignoriere eine .rs-Datei.";
        let candidates = extract_path_candidates(line);
        assert_eq!(
            candidates,
            vec![
                "Cargo.toml",
                "crates/memfuse-core/src/lib.rs",
                "physio_scheduler.rs"
            ]
        );
    }

    #[test]
    fn test_ignore_comment_convention() {
        let line = "| OFFEN-11 | `memfuse-db` | `physio_scheduler.rs` | <!-- doc-ref-ignore -->";
        let candidates = extract_path_candidates(line);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_existing_file_reference_passes() {
        let root = crate::find_root_dir();
        let content = "Siehe `Cargo.toml` und `AGENTS.md` für Konfiguration.";
        let violations = check_doc_references_in_content(content, "test.md", &root);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_non_existent_file_reference_fails() {
        let root = crate::find_root_dir();
        let content = "Feature implementiert in `nonexistent_module_xyz123.rs`.";
        let violations = check_doc_references_in_content(content, "test.md", &root);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].file, "test.md");
        assert_eq!(violations[0].line, 1);
        assert_eq!(
            violations[0].referenced_path,
            "nonexistent_module_xyz123.rs"
        );
    }
}
