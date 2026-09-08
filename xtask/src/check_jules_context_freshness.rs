use chrono::NaiveDate;
use regex::Regex;
use std::fs;
use std::path::Path;

use crate::find_root_dir;

#[derive(Debug, PartialEq, Eq)]
pub enum FreshnessError {
    MissingHeader {
        file: String,
    },
    MalformedDate {
        file: String,
        found: String,
    },
    StaleDate {
        file: String,
        found_date: NaiveDate,
        code_change_date: NaiveDate,
        days_behind: i64,
    },
}

pub fn check_file_freshness(
    file_rel_path: &str,
    content: &str,
    last_code_change_date: NaiveDate,
) -> Result<NaiveDate, FreshnessError> {
    let re = Regex::new(r"Stand:?\s*(\d{4}-\d{2}-\d{2})").map_err(|_| {
        FreshnessError::MissingHeader {
            file: file_rel_path.to_string(),
        }
    })?;

    let caps = match re.captures(content) {
        Some(c) => c,
        None => {
            return Err(FreshnessError::MissingHeader {
                file: file_rel_path.to_string(),
            })
        }
    };

    let date_str = &caps[1];
    let stand_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|_| {
        FreshnessError::MalformedDate {
            file: file_rel_path.to_string(),
            found: date_str.to_string(),
        }
    })?;

    let days_behind = (last_code_change_date - stand_date).num_days();
    if days_behind > 3 {
        return Err(FreshnessError::StaleDate {
            file: file_rel_path.to_string(),
            found_date: stand_date,
            code_change_date: last_code_change_date,
            days_behind,
        });
    }

    Ok(stand_date)
}

pub fn get_last_crates_change_date(root: &Path) -> Result<NaiveDate, String> {
    let output = std::process::Command::new("git")
        .args([
            "log",
            "-1",
            "--format=%cd",
            "--date=format:%Y-%m-%d",
            "--",
            "crates/",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("Failed to run git command: {}", e))?;

    if !output.status.success() {
        return Err("git log failed for crates/".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Err("No git history found for crates/".to_string());
    }

    NaiveDate::parse_from_str(&stdout, "%Y-%m-%d")
        .map_err(|e| format!("Failed to parse date '{}': {}", stdout, e))
}

pub fn run_check_jules_context_freshness() -> bool {
    println!("=== xtask check-jules-context-freshness ===");
    let root = find_root_dir();

    let code_change_date = match get_last_crates_change_date(&root) {
        Ok(date) => date,
        Err(e) => {
            eprintln!("❌ Failed to get last code change date for crates/: {}", e);
            eprintln!("=== xtask check-jules-context-freshness FAILED ===");
            return false;
        }
    };

    println!(
        "Last code change date in crates/: {}",
        code_change_date.format("%Y-%m-%d")
    );

    let files_to_check = ["AGENTS.md", ".jules/JULES_CONTEXT.md"];
    let mut failed = false;

    for file_rel_path in &files_to_check {
        let full_path = root.join(file_rel_path);
        let content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("❌ Failed to read {}: {}", file_rel_path, e);
                failed = true;
                continue;
            }
        };

        match check_file_freshness(file_rel_path, &content, code_change_date) {
            Ok(stand_date) => {
                println!(
                    "✅ {} is fresh (Stand: {}, crates/ change: {})",
                    file_rel_path,
                    stand_date.format("%Y-%m-%d"),
                    code_change_date.format("%Y-%m-%d")
                );
            }
            Err(FreshnessError::MissingHeader { file }) => {
                eprintln!("❌ [GATE-10]: Header 'Stand: YYYY-MM-DD' fehlt in {}", file);
                eprintln!(
                    "   Erwartetes Datum (letzter Code-Change in crates/): {}",
                    code_change_date.format("%Y-%m-%d")
                );
                eprintln!(
                    "   Behebung: Aktualisiere den Stand-Header in {} manuell auf das aktuelle Datum.",
                    file
                );
                failed = true;
            }
            Err(FreshnessError::MalformedDate { file, found }) => {
                eprintln!(
                    "❌ [GATE-10]: Stand-Datum '{}' in {} ist ungültig formatiert (erwartet: YYYY-MM-DD)",
                    found, file
                );
                eprintln!(
                    "   Erwartetes Datum (letzter Code-Change in crates/): {}",
                    code_change_date.format("%Y-%m-%d")
                );
                eprintln!(
                    "   Behebung: Aktualisiere den Stand-Header in {} manuell auf das aktuelle Datum.",
                    file
                );
                failed = true;
            }
            Err(FreshnessError::StaleDate {
                file,
                found_date,
                code_change_date,
                days_behind,
            }) => {
                eprintln!("❌ [GATE-10]: {} ist veraltet!", file);
                eprintln!(
                    "   Gefundenes Stand-Datum: {}",
                    found_date.format("%Y-%m-%d")
                );
                eprintln!(
                    "   Erwartetes Datum (letzter Code-Change in crates/): {}",
                    code_change_date.format("%Y-%m-%d")
                );
                eprintln!(
                    "   Differenz: {} Tage (erlaubter Schwellenwert: max 3 Tage)",
                    days_behind
                );
                eprintln!(
                    "   Behebung: Aktualisiere den Stand-Header in {} manuell auf das aktuelle Datum.",
                    file
                );
                failed = true;
            }
        }
    }

    if failed {
        eprintln!("=== xtask check-jules-context-freshness FAILED ===");
        false
    } else {
        println!("=== xtask check-jules-context-freshness PASSED ===");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_freshness_today() {
        let code_change = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        let content = "# AGENTS.md\n## Stand 2026-09-08\n";
        let res = check_file_freshness("AGENTS.md", content, code_change);
        assert_eq!(res, Ok(code_change));
    }

    #[test]
    fn test_freshness_within_tolerance() {
        let code_change = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        // 2 days before code change
        let content = "# AGENTS.md\n## Stand 2026-09-06\n";
        let res = check_file_freshness("AGENTS.md", content, code_change);
        assert_eq!(res, Ok(NaiveDate::from_ymd_opt(2026, 9, 6).unwrap()));
    }

    #[test]
    fn test_freshness_stale_exceeds_threshold() {
        let code_change = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        // 5 days before code change
        let content = "# AGENTS.md\n## Stand 2026-09-03\n";
        let res = check_file_freshness("AGENTS.md", content, code_change);
        assert_eq!(
            res,
            Err(FreshnessError::StaleDate {
                file: "AGENTS.md".to_string(),
                found_date: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
                code_change_date: code_change,
                days_behind: 5,
            })
        );
    }

    #[test]
    fn test_freshness_missing_header() {
        let code_change = NaiveDate::from_ymd_opt(2026, 9, 8).unwrap();
        let content = "# AGENTS.md\nNo date here\n";
        let res = check_file_freshness("AGENTS.md", content, code_change);
        assert_eq!(
            res,
            Err(FreshnessError::MissingHeader {
                file: "AGENTS.md".to_string()
            })
        );
    }
}
