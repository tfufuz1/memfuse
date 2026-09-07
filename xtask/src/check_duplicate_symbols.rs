//! Erkennt doppelte Top-Level-Symboldeklarationen (const, static, struct,
//! enum, fn, trait, type) innerhalb derselben Datei — ohne vollen
//! Compiler-Durchlauf. Dient als schnelles Vor-Gate vor `cargo check`,
//! um Merge-Kollisionen wie in diskann.rs (E0428, siehe Commits 307df50/
//! eb0e3ef) sofort mit exakter Zeilenangabe zu melden statt eines
//! generischen Workspace-Build-Fehlers.

use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DuplicateSymbol {
    pub file: String,
    pub symbol_kind: String, // "const", "struct", "fn", "enum", "trait", "static", "type"
    pub symbol_name: String,
    pub first_line: usize,
    pub duplicate_line: usize,
}

#[derive(Debug, Clone)]
struct SymbolOccurrence {
    line_number: usize,
    cfg_attr: Option<String>,
}

/// Scannt eine einzelne Datei auf doppelte Top-Level-Deklarationen.
/// Nutzt eine konservative Regex-Erkennung auf Zeilenebene, KEINE volle
/// Rust-Syntaxanalyse — bewusst leichtgewichtig, um schnell zu laufen.
/// Erkennt bewusst NUR Top-Level-Deklarationen (keine Einrückung), um
/// Fehlalarme bei gleichnamigen Symbolen in verschiedenen impl-Blöcken
/// oder Modulen zu vermeiden.
pub fn scan_file_for_duplicate_symbols(path: &Path) -> Result<Vec<DuplicateSymbol>, String> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return Err(format!("Failed to read file {}: {}", path.display(), e)),
    };

    let decl_re = Regex::new(
        r"^(pub(\([^)]+\))?\s+)?(const|static|struct|enum|fn|trait|type)\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .map_err(|e| format!("Invalid regex: {}", e))?;

    let mut occurrences: HashMap<(String, String), Vec<SymbolOccurrence>> = HashMap::new();

    let mut brace_depth: i32 = 0;
    let mut pending_cfg: Option<String> = None;
    let mut in_raw_str: Option<usize> = None;
    let mut in_str = false;

    let lines: Vec<&str> = content.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        // Check if line is an attribute like #[cfg(...)]
        if trimmed.starts_with("#[cfg(") && trimmed.ends_with(")]") {
            pending_cfg = Some(trimmed.to_string());
        }

        let is_inside_string = in_str || in_raw_str.is_some();

        // Only top-level declarations (brace_depth == 0, not in string, no leading indentation)
        if brace_depth == 0 && !is_inside_string && !line.is_empty() && line.trim_start() == *line {
            if let Some(caps) = decl_re.captures(line) {
                let symbol_kind = caps[3].to_string();
                let symbol_name = caps[4].to_string();

                if symbol_name != "_" {
                    let key = (symbol_kind, symbol_name);
                    let occ = SymbolOccurrence {
                        line_number: line_num,
                        cfg_attr: pending_cfg.clone(),
                    };

                    occurrences.entry(key).or_default().push(occ);
                }
            }
        }

        // Reset pending_cfg if line was not an attribute line or comment
        if !trimmed.starts_with("#[") && !trimmed.starts_with("//") && !trimmed.is_empty() {
            pending_cfg = None;
        }

        // Track state through characters
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if let Some(hashes) = in_raw_str {
                if chars[i] == '"' {
                    let mut match_hashes = 0;
                    while i + 1 + match_hashes < chars.len()
                        && chars[i + 1 + match_hashes] == '#'
                        && match_hashes < hashes
                    {
                        match_hashes += 1;
                    }
                    if match_hashes == hashes {
                        in_raw_str = None;
                        i += 1 + hashes;
                        continue;
                    }
                }
                i += 1;
            } else if in_str {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                } else if chars[i] == '"' {
                    in_str = false;
                }
                i += 1;
            } else {
                if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    break;
                }

                if chars[i] == 'r'
                    && i + 1 < chars.len()
                    && (chars[i + 1] == '"' || chars[i + 1] == '#')
                {
                    let mut h_count = 0;
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] == '#' {
                        h_count += 1;
                        j += 1;
                    }
                    if j < chars.len() && chars[j] == '"' {
                        in_raw_str = Some(h_count);
                        i = j + 1;
                        continue;
                    }
                }

                if chars[i] == '"' {
                    in_str = true;
                } else if chars[i] == '{' {
                    brace_depth += 1;
                } else if chars[i] == '}' {
                    brace_depth = brace_depth.saturating_sub(1);
                }
                i += 1;
            }
        }
    }

    let mut duplicates = Vec::new();
    let file_str = path.to_string_lossy().to_string();

    for ((symbol_kind, symbol_name), occ_list) in occurrences {
        if occ_list.len() > 1 {
            // Group occurrences by cfg attribute
            let mut cfg_groups: HashMap<Option<String>, Vec<usize>> = HashMap::new();
            for occ in occ_list {
                cfg_groups
                    .entry(occ.cfg_attr)
                    .or_default()
                    .push(occ.line_number);
            }

            for (_cfg, lines) in cfg_groups {
                if lines.len() > 1 {
                    let first_line = lines[0];
                    for &duplicate_line in &lines[1..] {
                        duplicates.push(DuplicateSymbol {
                            file: file_str.clone(),
                            symbol_kind: symbol_kind.clone(),
                            symbol_name: symbol_name.clone(),
                            first_line,
                            duplicate_line,
                        });
                    }
                }
            }
        }
    }

    duplicates.sort_by_key(|d| (d.file.clone(), d.first_line, d.duplicate_line));
    Ok(duplicates)
}

/// Scannt eine Liste von Dateien (z.B. aus `git diff --name-only`) und
/// gibt alle gefundenen Duplikate gesammelt zurück.
pub fn check_duplicate_symbols(files: &[String]) -> Result<Vec<DuplicateSymbol>, String> {
    let mut all_duplicates = Vec::new();

    let files_to_scan: Vec<String> = if files.is_empty() {
        let mut all_rs = Vec::new();
        for entry in WalkDir::new("crates")
            .into_iter()
            .chain(WalkDir::new("xtask").into_iter())
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("rs") {
                all_rs.push(p.to_string_lossy().to_string());
            }
        }
        all_rs
    } else {
        files
            .iter()
            .filter(|f| f.ends_with(".rs") && Path::new(f).exists())
            .cloned()
            .collect()
    };

    for file in files_to_scan {
        let path = Path::new(&file);
        if let Ok(dups) = scan_file_for_duplicate_symbols(path) {
            all_duplicates.extend(dups);
        }
    }

    Ok(all_duplicates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_detects_duplicate_const_same_name() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
pub const DISKANN_FOOTER_MAGIC: &[u8; 4] = b"FOOT";
pub const DISKANN_FOOTER_MAGIC: &[u8; 4] = b"FOOT";
"#
        )
        .unwrap();

        let dups = scan_file_for_duplicate_symbols(file.path()).unwrap();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].symbol_kind, "const");
        assert_eq!(dups[0].symbol_name, "DISKANN_FOOTER_MAGIC");
        assert_eq!(dups[0].first_line, 2);
        assert_eq!(dups[0].duplicate_line, 3);
    }

    #[test]
    fn test_no_false_positive_for_impl_methods_same_name() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
pub struct Foo;
pub struct Bar;

impl Foo {{
    pub fn new() -> Self {{
        Foo
    }}
}}

impl Bar {{
    pub fn new() -> Self {{
        Bar
    }}
}}
"#
        )
        .unwrap();

        let dups = scan_file_for_duplicate_symbols(file.path()).unwrap();
        assert!(
            dups.is_empty(),
            "Expected no duplicates for impl methods, found: {:?}",
            dups
        );
    }

    #[test]
    fn test_no_false_positive_across_cfg_feature_gates() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
#[cfg(feature = "a")]
pub const FOO: usize = 1;

#[cfg(feature = "b")]
pub const FOO: usize = 2;
"#
        )
        .unwrap();

        let dups = scan_file_for_duplicate_symbols(file.path()).unwrap();
        assert!(
            dups.is_empty(),
            "Expected no duplicates across different #[cfg(...)], found: {:?}",
            dups
        );
    }

    #[test]
    fn test_detects_duplicate_across_two_close_insertions_diskann_regression() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
use std::sync::Arc;

pub const DISKANN_FOOTER_MAGIC: &[u8; 4] = b"FOOT";
pub const DISKANN_INTEGRITY_KEY: &[u8; 16] = b"INTEGRITY_KEY_16";

// ... middle code ...

pub const DISKANN_FOOTER_MAGIC: &[u8; 4] = b"FOOT";
pub const DISKANN_INTEGRITY_KEY: &[u8; 16] = b"INTEGRITY_KEY_16";
"#
        )
        .unwrap();

        let dups = scan_file_for_duplicate_symbols(file.path()).unwrap();
        assert_eq!(dups.len(), 2);

        assert_eq!(dups[0].symbol_name, "DISKANN_FOOTER_MAGIC");
        assert_eq!(dups[0].first_line, 4);
        assert_eq!(dups[0].duplicate_line, 9);

        assert_eq!(dups[1].symbol_name, "DISKANN_INTEGRITY_KEY");
        assert_eq!(dups[1].first_line, 5);
        assert_eq!(dups[1].duplicate_line, 10);
    }
}
