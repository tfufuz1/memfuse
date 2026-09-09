use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq)]
pub struct AuditSection {
    pub file_path: String,
    pub header_line: usize,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DuplicationMatch {
    pub file_path: String,
    pub section_a_title: String,
    pub section_a_line: usize,
    pub section_b_title: String,
    pub section_b_line: usize,
    pub similarity: f64,
}

pub fn parse_audit_sections(content: &str, file_path: &str) -> Vec<AuditSection> {
    let mut sections = Vec::new();
    let mut current_title: Option<String> = None;
    let mut current_line = 0;
    let mut current_content = String::new();

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            if let Some(title) = current_title.take() {
                sections.push(AuditSection {
                    file_path: file_path.to_string(),
                    header_line: current_line,
                    title,
                    content: std::mem::take(&mut current_content),
                });
            }
            current_title = Some(trimmed.trim_start_matches('#').trim().to_string());
            current_line = idx + 1;
        } else if current_title.is_some() {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if let Some(title) = current_title {
        sections.push(AuditSection {
            file_path: file_path.to_string(),
            header_line: current_line,
            title,
            content: current_content,
        });
    }

    sections
}

pub fn normalize_sentence(sentence: &str) -> String {
    sentence
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn extract_sentence_set(content: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in content.lines() {
        let line_trimmed = line.trim();
        if line_trimmed.is_empty() || line_trimmed.starts_with("```") {
            continue;
        }
        for part in line_trimmed.split(". ") {
            let norm = normalize_sentence(part);
            if norm.len() >= 10 {
                set.insert(norm);
            }
        }
    }
    set
}

pub fn calculate_jaccard_similarity(set_a: &HashSet<String>, set_b: &HashSet<String>) -> f64 {
    if set_a.is_empty() || set_b.is_empty() {
        return 0.0;
    }
    let intersection = set_a.intersection(set_b).count();
    let union = set_a.union(set_b).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

pub fn check_sections_in_file(
    content: &str,
    file_path: &str,
    threshold: f64,
) -> Vec<DuplicationMatch> {
    let sections = parse_audit_sections(content, file_path);
    let mut matches = Vec::new();

    let sentence_sets: Vec<HashSet<String>> = sections
        .iter()
        .map(|sec| extract_sentence_set(&sec.content))
        .collect();

    for i in 0..sections.len() {
        if sentence_sets[i].len() < 3 {
            continue; // Ignore very small sections to avoid false positives on brief status notes
        }
        for j in (i + 1)..sections.len() {
            if sentence_sets[j].len() < 3 {
                continue;
            }

            let sim = calculate_jaccard_similarity(&sentence_sets[i], &sentence_sets[j]);
            if sim >= threshold {
                matches.push(DuplicationMatch {
                    file_path: file_path.to_string(),
                    section_a_title: sections[i].title.clone(),
                    section_a_line: sections[i].header_line,
                    section_b_title: sections[j].title.clone(),
                    section_b_line: sections[j].header_line,
                    similarity: sim,
                });
            }
        }
    }

    matches
}

pub fn find_audit_files(root: &Path) -> Vec<PathBuf> {
    let audits_dir = root.join("docs/audits");
    if !audits_dir.exists() {
        return Vec::new();
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(&audits_dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if name.starts_with("AUDIT_") && name.ends_with(".md") {
                    files.push(path.to_path_buf());
                }
            }
        }
    }
    files
}

pub fn run_check_audit_duplication(threshold: f64) -> Result<(), String> {
    println!(
        "=== Gate: check-audit-duplication (threshold = {:.0}%) ===",
        threshold * 100.0
    );

    let root = crate::find_root_dir();
    let audit_files = find_audit_files(&root);

    if audit_files.is_empty() {
        println!("No AUDIT_*.md files found in docs/audits/.");
        return Ok(());
    }

    let mut total_matches = Vec::new();

    for file_path in &audit_files {
        let rel_path = file_path
            .strip_prefix(&root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        if let Ok(content) = fs::read_to_string(file_path) {
            let matches = check_sections_in_file(&content, &rel_path, threshold);
            for m in matches {
                total_matches.push(m);
            }
        }
    }

    if total_matches.is_empty() {
        println!(
            "✅ check-audit-duplication: No audit section duplications found across {} files.",
            audit_files.len()
        );
        Ok(())
    } else {
        eprintln!(
            "❌ check-audit-duplication: Found {} duplicate section pair(s) exceeding {:.0}% similarity threshold!",
            total_matches.len(),
            threshold * 100.0
        );
        for m in &total_matches {
            eprintln!(
                "  - File: {}\n    Section A: \"{}\" (line {})\n    Section B: \"{}\" (line {})\n    Similarity: {:.1}%\n",
                m.file_path,
                m.section_a_title,
                m.section_a_line,
                m.section_b_title,
                m.section_b_line,
                m.similarity * 100.0
            );
        }
        Err(format!(
            "{} audit section duplicate pair(s) detected.",
            total_matches.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_audit_sections() {
        let markdown = r#"# Document Title

Intro text

## 1. Executive Summary

Summary content line 1.
Summary content line 2.

## 2. Inventory Check

Inventory line 1.
"#;
        let sections = parse_audit_sections(markdown, "test.md");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, "1. Executive Summary");
        assert_eq!(sections[0].header_line, 5);
        assert!(sections[0].content.contains("Summary content line 1."));
        assert_eq!(sections[1].title, "2. Inventory Check");
        assert_eq!(sections[1].header_line, 10);
    }

    #[test]
    fn test_duplicate_section_detection_fixture() {
        let markdown_with_duplicate = r#"
## 11. Tier 1 Deep Audit & Verification (2026-09-09 — Task JULES-20260909-DEEP)

- Property-Based Tests (`proptest`): 11/11 proptests green in test harness.
- Concurrency Stress Runs: 10/10 iterations with test-threads=8 passed without panics.
- TxId Boundary Exhaustion Simulation: verified controlled MemFuseError Transaction returns.
- SnapshotRegistry GC Race Stress: verified zero race conditions under concurrent access.
- Quality Gate Stack: 156 unit + 2 integration + 5 robustness tests passing green.

## 14. Tier 1 Deep Audit & Verification Pass (2026-09-09 — SESSION 4b5ed819 / Task JULES-20260909-DEEP)

- Property-Based Tests (`proptest`): 11/11 proptests green in test harness.
- Concurrency Stress Runs: 10/10 iterations with test-threads=8 passed without panics.
- TxId Boundary Exhaustion Simulation: verified controlled MemFuseError Transaction returns.
- SnapshotRegistry GC Race Stress: verified zero race conditions under concurrent access.
- Quality Gate Stack: 156 unit + 2 integration + 5 robustness tests passing green.
"#;

        let matches = check_sections_in_file(markdown_with_duplicate, "test.md", 0.85);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].section_a_title,
            "11. Tier 1 Deep Audit & Verification (2026-09-09 — Task JULES-20260909-DEEP)"
        );
        assert_eq!(
            matches[0].section_b_title,
            "14. Tier 1 Deep Audit & Verification Pass (2026-09-09 — SESSION 4b5ed819 / Task JULES-20260909-DEEP)"
        );
        assert!(matches[0].similarity >= 0.85);
    }

    #[test]
    fn test_distinct_sections_not_flagged() {
        let markdown_clean = r#"
## 11. Tier 1 Deep Audit & Verification — konsolidiert (2026-09-09)

- Property-Based Tests (`proptest`): 11/11 proptests green in test harness.
- Concurrency Stress Runs: 10/10 iterations in Session 96e5c38b sowie 5/5 in Session 4b5ed819.
- TxId Boundary Exhaustion Simulation: verified controlled MemFuseError Transaction returns.
- SnapshotRegistry GC Race Stress: verified zero race conditions under concurrent access.

## 14. [Konsolidiert in §11 — keine neue Prüftiefe gegenüber Session 96e5c38b identifiziert]
"#;

        let matches = check_sections_in_file(markdown_clean, "test.md", 0.85);
        assert!(matches.is_empty());
    }
}
