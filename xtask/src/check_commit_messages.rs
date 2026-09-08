// MemFuse — Commit-Message Validation & CI Gate (Gate 14)
//
// Prüft, ob neue Commits im aktuellen Branch/PR gegenüber dem Base-Branch
// aussagekräftige Commit-Messages gemäß Conventional Commits enthalten.

use regex::Regex;
use std::env;
use std::process::Command;

pub fn validate_commit_message(msg: &str) -> Result<(), String> {
    let trimmed = msg.trim();
    let lower = trimmed.to_lowercase();

    let blocklist = [
        "shell-commit",
        "wip",
        "update",
        "fix",
        "misc",
        "changes",
        "commit",
    ];

    if blocklist.contains(&lower.as_str()) {
        return Err(format!(
            "Commit-Message '{}' ist in der Blockliste generischer/nichtssagender Nachrichten.",
            trimmed
        ));
    }

    let re = Regex::new(
        r"^(feat|fix|docs|refactor|test|chore|bench|perf|style|build|ci)(\([a-z0-9_-]+\))?:\s.+",
    )
    .map_err(|e| format!("Regex-Fehler: {e}"))?;

    if !re.is_match(trimmed) {
        return Err(format!(
            "Commit-Message '{}' entspricht nicht dem Conventional-Commits-Schema `<type>(<scope>): <description>`",
            trimmed
        ));
    }

    if let Some((_type_scope, description)) = trimmed.split_once(':') {
        let desc_trimmed = description.trim();
        if desc_trimmed.chars().count() < 15 {
            return Err(format!(
                "Text nach dem Doppelpunkt in Commit-Message '{}' ist zu kurz (mindestens 15 Zeichen erforderlich, erhalten: {})",
                trimmed,
                desc_trimmed.chars().count()
            ));
        }
    }

    Ok(())
}

pub fn check_commit_messages() -> Result<(), String> {
    println!("=== Running xtask check-commit-messages ===");

    let base_ref = env::var("MEMFUSE_CI_BASE_REF")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "origin/main".to_string());

    let range = format!("{}..HEAD", base_ref);
    let output = Command::new("git")
        .args(["log", "--format=%H%x09%s", &range])
        .output()
        .map_err(|e| format!("git log Aufruf fehlgeschlagen: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git log Command für Range '{}' fehlgeschlagen: {}",
            range, stderr
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut violations = Vec::new();

    for line in stdout.lines() {
        let line_trimmed = line.trim();
        if line_trimmed.is_empty() {
            continue;
        }

        if let Some((hash, subject)) = line_trimmed.split_once('\t') {
            let short_hash = &hash[..std::cmp::min(7, hash.len())];
            if let Err(err) = validate_commit_message(subject) {
                violations.push((short_hash.to_string(), subject.to_string(), err));
            }
        }
    }

    if violations.is_empty() {
        println!("✅ Alle geprüften Commit-Messages entsprechen dem Conventional-Commits-Schema.");
        Ok(())
    } else {
        eprintln!(
            "❌ GATE-FEHLER: Es wurden aussageschwache oder ungültige Commit-Messages gefunden!"
        );
        eprintln!();
        eprintln!("Anzahl Verstöße: {}", violations.len());
        eprintln!();
        for (short_hash, subject, reason) in &violations {
            eprintln!("  Commit {}: \"{}\"", short_hash, subject);
            eprintln!("    Ursache: {}", reason);
        }
        eprintln!();
        eprintln!("ERWARTETES SCHEMA:");
        eprintln!("  <type>(<scope>): <description>");
        eprintln!("  - Valid Typen: feat, fix, docs, refactor, test, chore, bench, perf, style, build, ci");
        eprintln!("  - Mindestlänge der Description: 15 Zeichen nach ': '");
        eprintln!("  - Keine generischen Messages (z.B. 'Shell-Commit', 'wip', 'update', etc.)");
        eprintln!();
        eprintln!("WARUM GENERISCHE MESSAGES VERBOTEN SIND:");
        eprintln!(
            "  Bei agentengesteuerter Entwicklung (Google Jules als alleiniger Committer ohne"
        );
        eprintln!(
            "  menschlichen Reviewer im Loop) ist eine präzise Audit-Trail-Integrität essenziell,"
        );
        eprintln!(
            "  damit nachfolgende Agenten-Sessions die Historie ohne aufwändige Diff-Analysen"
        );
        eprintln!("  nachvollziehen können.");

        Err(format!(
            "Commit-Message Validation fehlgeschlagen: {} ungültige(s) Commit(s)",
            violations.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_commit_messages() {
        assert!(
            validate_commit_message("feat(graph): add PPR bidirectional traversal support").is_ok()
        );
        assert!(validate_commit_message("chore(ci): add commit message linter gate").is_ok());
        assert!(validate_commit_message("fix: resolve memory leak in buffer pool").is_ok());
        assert!(
            validate_commit_message("docs(adr): document architecture decisions for WAL").is_ok()
        );
    }

    #[test]
    fn test_blocklist_commit_messages() {
        assert!(validate_commit_message("Shell-Commit").is_err());
        assert!(validate_commit_message("shell-commit").is_err());
        assert!(validate_commit_message("wip").is_err());
        assert!(validate_commit_message("WIP").is_err());
        assert!(validate_commit_message("update").is_err());
        assert!(validate_commit_message("fix").is_err());
        assert!(validate_commit_message("misc").is_err());
        assert!(validate_commit_message("changes").is_err());
        assert!(validate_commit_message("commit").is_err());
    }

    #[test]
    fn test_short_description_commit_messages() {
        let res = validate_commit_message("fix: x");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("zu kurz"));

        let res2 = validate_commit_message("feat(core): 12345");
        assert!(res2.is_err());
        assert!(res2.unwrap_err().contains("zu kurz"));
    }

    #[test]
    fn test_invalid_schema_commit_messages() {
        let res = validate_commit_message("update stuff");
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Conventional-Commits-Schema"));

        let res2 = validate_commit_message("invalid_type: doing something");
        assert!(res2.is_err());

        let res3 = validate_commit_message("feat:no space after colon");
        assert!(res3.is_err());
    }
}
