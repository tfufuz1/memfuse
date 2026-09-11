use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct MutationScoreEntry {
    pub date: String,
    pub commit: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub total_mutants: usize,
    pub caught: usize,
    pub missed: usize,
    pub score_pct: f64,
}

pub fn run_record_mutation_score(args: &[String], root: &Path) -> Result<(), String> {
    let mut crate_name = String::new();
    let mut total: Option<usize> = None;
    let mut caught: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--crate" => {
                if i + 1 < args.len() {
                    crate_name = args[i + 1].clone();
                    i += 1;
                }
            }
            "--total" => {
                if i + 1 < args.len() {
                    total = args[i + 1].parse::<usize>().ok();
                    i += 1;
                }
            }
            "--caught" => {
                if i + 1 < args.len() {
                    caught = args[i + 1].parse::<usize>().ok();
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if crate_name.is_empty() {
        return Err("Missing or empty required argument --crate <name>".to_string());
    }
    let total = total.ok_or_else(|| "Missing or invalid argument --total <N>".to_string())?;
    let caught = caught.ok_or_else(|| "Missing or invalid argument --caught <N>".to_string())?;

    if caught > total {
        return Err(format!(
            "Invalid counts: caught ({caught}) cannot exceed total ({total})"
        ));
    }

    let missed = total.saturating_sub(caught);
    let score_pct = if total > 0 {
        let raw = (caught as f64 / total as f64) * 100.0;
        (raw * 100.0).round() / 100.0
    } else {
        0.0
    };

    let today = Utc::now().format("%Y-%m-%d").to_string();
    let commit_sha = match Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    };

    let new_entry = MutationScoreEntry {
        date: today,
        commit: commit_sha,
        crate_name,
        total_mutants: total,
        caught,
        missed,
        score_pct,
    };

    let docs_dir = root.join("docs");
    if !docs_dir.exists() {
        fs::create_dir_all(&docs_dir)
            .map_err(|e| format!("Failed to create docs directory: {e}"))?;
    }

    let history_file = docs_dir.join("mutation_score_history.jsonl");

    let mut entries: Vec<MutationScoreEntry> = Vec::new();
    let mut updated = false;

    if history_file.exists() {
        let file = fs::File::open(&history_file)
            .map_err(|e| format!("Failed to open history file {}: {e}", history_file.display()))?;
        let reader = BufReader::new(file);

        for (line_num, line_res) in reader.lines().enumerate() {
            let line = line_res.map_err(|e| {
                format!("Failed to read line {} in history file: {e}", line_num + 1)
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(existing) = serde_json::from_str::<MutationScoreEntry>(trimmed) {
                if existing.date == new_entry.date
                    && existing.commit == new_entry.commit
                    && existing.crate_name == new_entry.crate_name
                {
                    entries.push(new_entry.clone());
                    updated = true;
                } else {
                    entries.push(existing);
                }
            } else {
                return Err(format!(
                    "Invalid JSON line in {}:{}",
                    history_file.display(),
                    line_num + 1
                ));
            }
        }
    }

    if !updated {
        entries.push(new_entry.clone());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&history_file)
        .map_err(|e| {
            format!(
                "Failed to open history file {} for writing: {e}",
                history_file.display()
            )
        })?;

    for entry in &entries {
        let line = serde_json::to_string(entry)
            .map_err(|e| format!("Serialization error for score entry: {e}"))?;
        writeln!(file, "{}", line)
            .map_err(|e| format!("Failed to write line to history file: {e}"))?;
    }

    println!(
        "📊 Mutation score recorded for {}: {}/{} ({:.2}%) -> {}",
        new_entry.crate_name,
        new_entry.caught,
        new_entry.total_mutants,
        new_entry.score_pct,
        history_file.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_record_mutation_score_success_and_idempotency() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let args1 = vec![
            "--crate".to_string(),
            "memfuse-test".to_string(),
            "--total".to_string(),
            "100".to_string(),
            "--caught".to_string(),
            "85".to_string(),
        ];

        assert!(run_record_mutation_score(&args1, root).is_ok());

        let history_file = root.join("docs/mutation_score_history.jsonl");
        assert!(history_file.exists());

        let content = fs::read_to_string(&history_file).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let parsed: MutationScoreEntry = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.crate_name, "memfuse-test");
        assert_eq!(parsed.total_mutants, 100);
        assert_eq!(parsed.caught, 85);
        assert_eq!(parsed.missed, 15);
        assert_eq!(parsed.score_pct, 85.0);

        // Run again with updated counts for the same commit/crate -> should update the existing entry (idempotent)
        let args2 = vec![
            "--crate".to_string(),
            "memfuse-test".to_string(),
            "--total".to_string(),
            "100".to_string(),
            "--caught".to_string(),
            "90".to_string(),
        ];
        assert!(run_record_mutation_score(&args2, root).is_ok());

        let content2 = fs::read_to_string(&history_file).unwrap();
        let lines2: Vec<&str> = content2.lines().collect();
        assert_eq!(lines2.len(), 1);

        let parsed2: MutationScoreEntry = serde_json::from_str(lines2[0]).unwrap();
        assert_eq!(parsed2.caught, 90);
        assert_eq!(parsed2.missed, 10);
        assert_eq!(parsed2.score_pct, 90.0);
    }

    #[test]
    fn test_record_mutation_score_invalid_args() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Caught > Total
        let args = vec![
            "--crate".to_string(),
            "memfuse-test".to_string(),
            "--total".to_string(),
            "10".to_string(),
            "--caught".to_string(),
            "15".to_string(),
        ];
        assert!(run_record_mutation_score(&args, root).is_err());

        // Missing total
        let args_no_total = vec![
            "--crate".to_string(),
            "memfuse-test".to_string(),
            "--caught".to_string(),
            "5".to_string(),
        ];
        assert!(run_record_mutation_score(&args_no_total, root).is_err());
    }
}
