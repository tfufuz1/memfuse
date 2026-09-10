//! Gate: Prüft ob im Commit-Body genannte Dateinamen tatsächlich im Diff vorhanden sind.
//! Verhindert "Phantom-Verifikation" (Audit 4.4).

use regex::Regex;
use std::collections::HashSet;
use std::process::Command;

/// Extrahiert alle `.rs`-Dateinamen aus einem Text (Commit-Message oder PR-Body).
fn extract_claimed_rs_files(text: &str) -> HashSet<String> {
    let re = Regex::new(r"\b([\w/.-]+\.rs)\b").unwrap();
    re.captures_iter(text).map(|c| c[1].to_string()).collect()
}

/// Gibt die tatsächlich geänderten Dateien im aktuellen Branch zurück.
fn get_actual_diff_files(base_ref: &str) -> Vec<String> {
    let output = Command::new("git")
        .args(["diff", "--name-only", base_ref, "HEAD"])
        .output()
        .expect("git diff fehlgeschlagen");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Liest die letzten N Commit-Messages.
fn get_recent_commit_messages(n: usize) -> Vec<(String, String)> {
    let output = Command::new("git")
        .args([
            "log",
            &format!("-{}", n),
            "--format=%H%n%B%n---COMMIT-SEP---",
        ])
        .output()
        .expect("git log fehlgeschlagen");
    let text = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();
    for block in text.split("---COMMIT-SEP---") {
        let mut lines = block.trim().lines();
        if let Some(hash) = lines.next() {
            let body = lines.collect::<Vec<_>>().join("\n");
            if !hash.is_empty() {
                results.push((hash.trim().to_string(), body));
            }
        }
    }
    results
}

pub fn run_check_phantom_files() -> bool {
    let base_ref =
        std::env::var("MEMFUSE_CI_BASE_REF").unwrap_or_else(|_| "origin/main".to_string());

    println!("=== Gate: check-phantom-files (Basis: {}) ===", base_ref);

    let actual_files: HashSet<String> = get_actual_diff_files(&base_ref)
        .into_iter()
        .map(|f| {
            // Basename extrahieren für flexiblen Match
            std::path::Path::new(&f)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    // PR-Body aus Env (gesetzt durch CI)
    let pr_body = std::env::var("MEMFUSE_PR_BODY").unwrap_or_default();

    // Letzte 3 Commit-Messages
    let commits = get_recent_commit_messages(3);

    let mut phantoms: Vec<(String, String)> = Vec::new();

    // PR-Body prüfen
    for claimed in extract_claimed_rs_files(&pr_body) {
        let basename = std::path::Path::new(&claimed)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !actual_files.contains(&basename) && !actual_files.contains(&claimed) {
            phantoms.push(("PR-Body".to_string(), claimed));
        }
    }

    // Commit-Messages prüfen
    for (hash, body) in &commits {
        // Nur bei Schlüsselwörtern prüfen (Heuristik gegen False Positives)
        if body.contains("erstellt")
            || body.contains("hinzugefügt")
            || body.contains("added")
            || body.contains("created")
        {
            for claimed in extract_claimed_rs_files(body) {
                let basename = std::path::Path::new(&claimed)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if !actual_files.contains(&basename) && !actual_files.contains(&claimed) {
                    let short_hash = if hash.len() >= 8 {
                        &hash[..8]
                    } else {
                        hash.as_str()
                    };
                    phantoms.push((format!("Commit {}", short_hash), claimed));
                }
            }
        }
    }

    if phantoms.is_empty() {
        println!("✅ Keine Phantom-Dateien gefunden.");
        true
    } else {
        eprintln!(
            "❌ Phantom-Verifikation erkannt — folgende Dateien werden behauptet, sind aber nicht im Diff:"
        );
        for (source, file) in &phantoms {
            eprintln!("   {}: {}", source, file);
        }
        eprintln!("Tatsächliche .rs-Dateien im Diff: {:?}", actual_files);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_claimed_rs_files() {
        let text = "feat: added concurrency_stress.rs with 50 tests and src/lib.rs update";
        let claimed = extract_claimed_rs_files(text);
        assert!(claimed.contains("concurrency_stress.rs"));
        assert!(claimed.contains("src/lib.rs"));
    }
}
