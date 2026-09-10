use regex::Regex;
use std::collections::HashSet;
use std::env;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct CommitEntry {
    pub hash: String,
    pub date: String,
    pub raw_message: String,
    pub scope: String,
    pub subject: String,
    pub normalized_subject: String,
}

pub fn normalize_subject(subject: &str) -> String {
    let lower = subject.to_lowercase();
    // Remove ADR references like (ADR-067) or ADR-067
    let adr_re = Regex::new(r"(?i)\(?adr-\d+\)?").unwrap();
    let no_adr = adr_re.replace_all(&lower, "");

    // Replace non-alphabetic characters with space
    let clean: String = no_adr
        .chars()
        .map(|c| if c.is_alphabetic() { c } else { ' ' })
        .collect();

    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_conventional_commit(raw_message: &str) -> (String, String) {
    let re =
        Regex::new(r"^(?i)[a-z0-9_-]+(?:\((?P<scope>[^)]+)\))?!?:\s*(?P<subject>.*)$").unwrap();
    if let Some(caps) = re.captures(raw_message.trim()) {
        let scope = caps
            .name("scope")
            .map(|m| m.as_str().trim().to_lowercase())
            .unwrap_or_default();
        let subject = caps
            .name("subject")
            .map(|m| m.as_str().trim())
            .unwrap_or_default()
            .to_string();
        (scope, subject)
    } else {
        (String::new(), raw_message.trim().to_string())
    }
}

pub fn get_trigrams(s: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    if s.is_empty() {
        return set;
    }
    let padded = format!("  {}  ", s);
    let chars: Vec<char> = padded.chars().collect();
    if chars.len() >= 3 {
        for window in chars.windows(3) {
            set.insert(window.iter().collect::<String>());
        }
    }
    set
}

pub fn normalized_similarity(a: &str, b: &str) -> f64 {
    let norm_a = normalize_subject(a);
    let norm_b = normalize_subject(b);

    if norm_a.is_empty() && norm_b.is_empty() {
        return 1.0;
    }
    if norm_a.is_empty() || norm_b.is_empty() {
        return 0.0;
    }

    let tri_a = get_trigrams(&norm_a);
    let tri_b = get_trigrams(&norm_b);

    let intersection = tri_a.intersection(&tri_b).count();
    let union = tri_a.union(&tri_b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f64 / union as f64
}

pub fn parse_override_exception(pr_body: &str) -> Option<String> {
    let re =
        Regex::new(r"(?i)(?:Supersedes|Fixes-Regression-Of):\s*\b([a-fA-F0-9]{7,40})\b").unwrap();
    re.captures(pr_body).map(|caps| caps[1].to_string())
}

pub fn parse_git_log_output(output: &str) -> Vec<CommitEntry> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let hash = parts[0].trim().to_string();
            let date = parts[1].trim().to_string();
            let raw_message = parts[2].trim().to_string();

            let (scope, subject) = parse_conventional_commit(&raw_message);
            let normalized_subject = normalize_subject(&subject);

            entries.push(CommitEntry {
                hash,
                date,
                raw_message,
                scope,
                subject,
                normalized_subject,
            });
        }
    }
    entries
}

pub fn get_historical_commits() -> Result<Vec<CommitEntry>, String> {
    let output = Command::new("git")
        .args(["log", "--since=30 days ago", "--format=%H%x09%ci%x09%s"])
        .output()
        .map_err(|e| format!("Failed to execute git log: {}", e))?;

    if !output.status.success() {
        return Err("git log command returned non-zero exit status".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_git_log_output(&stdout))
}

pub fn get_new_commits() -> Result<Vec<CommitEntry>, String> {
    let output = Command::new("git")
        .args(["log", "origin/main..HEAD", "--format=%H%x09%ci%x09%s"])
        .output();

    let stdout = match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if s.trim().is_empty() {
                // Fallback to last commit if origin/main..HEAD is empty
                let fallback = Command::new("git")
                    .args(["log", "-1", "--format=%H%x09%ci%x09%s"])
                    .output()
                    .map_err(|e| format!("Failed to execute git log fallback: {}", e))?;
                String::from_utf8_lossy(&fallback.stdout).to_string()
            } else {
                s
            }
        }
        _ => {
            let fallback = Command::new("git")
                .args(["log", "-1", "--format=%H%x09%ci%x09%s"])
                .output()
                .map_err(|e| format!("Failed to execute git log fallback: {}", e))?;
            String::from_utf8_lossy(&fallback.stdout).to_string()
        }
    };

    Ok(parse_git_log_output(&stdout))
}

#[derive(Debug, PartialEq)]
pub struct DuplicateHit {
    pub new_commit: CommitEntry,
    pub historical_commit: CommitEntry,
    pub similarity: f64,
}

pub fn check_duplicate_intent() -> Result<(), String> {
    println!("=== Gate 12: Duplicate-PR-Intent-Detection ===");

    let historical_commits = get_historical_commits()?;
    let new_commits = get_new_commits()?;

    let pr_body = env::var("MEMFUSE_PR_BODY").unwrap_or_default();
    let mut override_hash = parse_override_exception(&pr_body);

    if override_hash.is_none() {
        for new_c in &new_commits {
            if let Some(h) = parse_override_exception(&new_c.raw_message) {
                override_hash = Some(h);
                break;
            }
        }
    }

    let mut hits = Vec::new();

    for new_c in &new_commits {
        if new_c.normalized_subject.is_empty() {
            continue;
        }

        for hist_c in &historical_commits {
            if new_c.hash == hist_c.hash {
                continue;
            }

            if new_c.scope == hist_c.scope {
                let similarity = normalized_similarity(&new_c.subject, &hist_c.subject);
                if similarity >= 0.85 {
                    hits.push(DuplicateHit {
                        new_commit: new_c.clone(),
                        historical_commit: hist_c.clone(),
                        similarity,
                    });
                }
            }
        }
    }

    if hits.is_empty() {
        println!("✅ Gate 12: No duplicate PR intent detected.");
        Ok(())
    } else {
        if let Some(ref ref_hash) = override_hash {
            println!(
                "ℹ️ Gate 12: Potential duplicate intent detected, but valid override exception found in PR body (Supersedes/Fixes-Regression-Of: {}).",
                ref_hash
            );
            for hit in &hits {
                println!(
                    "  [INFO] New commit '{}' ({}) is {:.2}% similar to historical commit '{}' ({}) [scope: '{}']",
                    hit.new_commit.raw_message,
                    hit.new_commit.hash,
                    hit.similarity * 100.0,
                    hit.historical_commit.raw_message,
                    hit.historical_commit.hash,
                    hit.new_commit.scope
                );
            }
            Ok(())
        } else {
            eprintln!("❌ Gate 12: Potential duplicate PR intent detected without valid 'Supersedes:' or 'Fixes-Regression-Of:' reference!");
            for hit in &hits {
                eprintln!(
                    "  - New Commit: {} ({})",
                    hit.new_commit.raw_message, hit.new_commit.hash
                );
                eprintln!(
                    "    Matches Historical Commit: {} ({})",
                    hit.historical_commit.raw_message, hit.historical_commit.hash
                );
                eprintln!(
                    "    Scope: '{}', Similarity: {:.2}%",
                    hit.new_commit.scope,
                    hit.similarity * 100.0
                );
            }
            eprintln!("\n💡 To override if this intentional re-implementation/refactor, add 'Supersedes: <commit-hash>' or 'Fixes-Regression-Of: <commit-hash>' to your PR description.");
            Err("Duplicate PR intent detected".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_intent_similarity() {
        let a = "calibrate PathRAG sufficiency threshold to 0.1";
        let b = "calibrate PathRAG sufficiency threshold to 0.1";
        let sim = normalized_similarity(a, b);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Expected ~1.0 similarity, got {}",
            sim
        );
    }

    #[test]
    fn test_identical_intent_with_adr_similarity() {
        let a = "calibrate PathRAG sufficiency threshold to 0.1 (ADR-067)";
        let b = "calibrate PathRAG sufficiency threshold to 0.1";
        let sim = normalized_similarity(a, b);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Expected ~1.0 similarity despite ADR reference, got {}",
            sim
        );
    }

    #[test]
    fn test_different_intent_similarity() {
        let a = "calibrate PathRAG sufficiency threshold to 0.1";
        let b = "implement bidirectional PPR damping factor";
        let sim = normalized_similarity(a, b);
        assert!(
            sim < 0.85,
            "Expected similarity significantly lower than 0.85, got {}",
            sim
        );
    }

    #[test]
    fn test_parse_override_exception_valid() {
        let pr_body = "This PR improves PPR damping.\n\nSupersedes: a1b2c3d\n";
        let parsed = parse_override_exception(pr_body);
        assert_eq!(parsed, Some("a1b2c3d".to_string()));

        let pr_body_2 = "Fixes-Regression-Of: 4b8a9c0d1e2f3a4b5c6d7e8f";
        let parsed_2 = parse_override_exception(pr_body_2);
        assert_eq!(parsed_2, Some("4b8a9c0d1e2f3a4b5c6d7e8f".to_string()));
    }

    #[test]
    fn test_parse_override_exception_without_valid_hash() {
        let pr_body = "This PR supersedes prior work.\nSupersedes:\nFixes-Regression-Of: xyz";
        let parsed = parse_override_exception(pr_body);
        assert_eq!(
            parsed, None,
            "Empty or non-hex hash after keyword must not be accepted"
        );

        let pr_body_short_hash = "Supersedes: 123456"; // 6 chars < 7 chars
        let parsed_short = parse_override_exception(pr_body_short_hash);
        assert_eq!(
            parsed_short, None,
            "Hash shorter than 7 chars must not be accepted"
        );
    }

    #[test]
    fn test_parse_conventional_commit() {
        let msg = "feat(memfuse-core): add new vector index";
        let (scope, subject) = parse_conventional_commit(msg);
        assert_eq!(scope, "memfuse-core");
        assert_eq!(subject, "add new vector index");

        let msg_no_scope = "fix: resolve deadlock in store";
        let (scope_none, subject_none) = parse_conventional_commit(msg_no_scope);
        assert_eq!(scope_none, "");
        assert_eq!(subject_none, "resolve deadlock in store");
    }
}
