// MemFuse — ADR Deprecation & Removal Deadline CI Gate
//
// Überprüft `DECISIONS.md` auf ADRs mit definierten Deprecation-/Removal-Fristen.
//
// Prüflogik:
// 1. Parst `DECISIONS.md` nach ADR-Einträgen mit `Removal Deadline`, `Deprecation Deadline` oder `Review Deadline`.
// 2. Prüft das Zieldatum gegen das aktuelle Systemdatum.
// 3. Frist in der Zukunft (0 <= Resttage <= 14): Warnung zur rechtzeitigen Vorbereitung.
// 4. Frist in der Vergangenheit (< 0 Tage):
//    - Falls das Ziel-Crate / die Ziel-Datei (z. B. `crates/memfuse-tauri`) weiterhin existiert:
//      Harter CI-Fehler (Exit-Code != 0) mit klarer Handlungsaufforderung (physisch entfernen oder Frist per neuem ADR verlängern).
//    - Falls der Zielpfad bereits entfernt wurde: Kein Fehler (Aufgabe bereits erledigt).

use chrono::NaiveDate;
use std::fs;
use std::path::Path;

pub const ADR_DEADLINE_WARNING_THRESHOLD_DAYS: i64 = 14;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AdrDeadlineEntry {
    pub adr_id: String,
    pub title: String,
    pub status: String,
    pub deadline: Option<String>,
    pub target_path: Option<String>,
}

impl AdrDeadlineEntry {
    pub fn title_summary(&self) -> &str {
        self.title.trim()
    }
}

pub fn parse_adr_deadlines(content: &str) -> Result<Vec<AdrDeadlineEntry>, String> {
    let mut entries = Vec::new();
    let blocks = content.split("\n# ");

    for (idx, block) in blocks.enumerate() {
        let block_str = if idx > 0 {
            format!("# {block}")
        } else {
            block.to_string()
        };

        if !block_str.contains("# ADR-") {
            continue;
        }

        let mut adr_id = String::new();
        let mut title = String::new();
        let mut status = String::new();
        let mut deadline = None;
        let mut target_path = None;

        for line in block_str.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("# ADR-") {
                let header = trimmed.trim_start_matches("# ").trim();
                if let Some((id, rest)) = header.split_once(':') {
                    adr_id = id.trim().to_string();
                    title = rest.trim().to_string();
                } else {
                    adr_id = header.to_string();
                }
            } else if trimmed.starts_with("* **Status:**") || trimmed.starts_with("**Status:**") {
                status = trimmed
                    .trim_start_matches("*")
                    .trim_start_matches("**Status:**")
                    .trim()
                    .to_string();
            } else if trimmed.contains("Removal Deadline:")
                || trimmed.contains("Deprecation Deadline:")
                || trimmed.contains("Review Deadline:")
            {
                let raw_val = if let Some((_, val)) = trimmed.split_once("Deadline:") {
                    val.trim().trim_matches('*').trim().to_string()
                } else {
                    String::new()
                };
                if !raw_val.is_empty() {
                    deadline = Some(raw_val);
                }
            } else if trimmed.contains("Target Path:") || trimmed.contains("Target Crate:") {
                let raw_val = if let Some((_, val)) = trimmed.split_once(":") {
                    val.trim()
                        .trim_matches('*')
                        .trim()
                        .trim_matches('`')
                        .to_string()
                } else {
                    String::new()
                };
                if !raw_val.is_empty() {
                    target_path = Some(raw_val);
                }
            }
        }

        if !adr_id.is_empty() && deadline.is_some() {
            entries.push(AdrDeadlineEntry {
                adr_id,
                title,
                status,
                deadline,
                target_path,
            });
        }
    }

    Ok(entries)
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct AdrDeadlineCheckResult {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub fn check_adr_deadlines_at(
    entries: &[AdrDeadlineEntry],
    today_str: &str,
    root_dir: &Path,
) -> AdrDeadlineCheckResult {
    let mut result = AdrDeadlineCheckResult::default();
    let today = match NaiveDate::parse_from_str(today_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return result,
    };

    for entry in entries {
        if let Some(due_str) = &entry.deadline {
            if let Ok(due_date) = NaiveDate::parse_from_str(due_str, "%Y-%m-%d") {
                let days_until_due = (due_date - today).num_days();
                let target_desc = entry
                    .target_path
                    .as_deref()
                    .unwrap_or("kein Zielpfad angegeben");

                if days_until_due < 0 {
                    let target_exists = entry
                        .target_path
                        .as_ref()
                        .map(|p| root_dir.join(p).exists())
                        .unwrap_or(true);

                    if target_exists {
                        result.errors.push(format!(
                            "❌ ADR-FRIST ÜBERSCHRITTEN: {} ('{}') — Entfernungs-/Review-Frist war am {}, aber '{}' existiert weiterhin im Repository. Bitte physisch entfernen oder Frist per neuem ADR verlängern.",
                            entry.adr_id,
                            entry.title_summary(),
                            due_str,
                            target_desc
                        ));
                    }
                } else if days_until_due <= ADR_DEADLINE_WARNING_THRESHOLD_DAYS {
                    result.warnings.push(format!(
                        "⚠️ WARNUNG: ADR-FRIST {} ('{}'): Deprecation-/Removal-Frist {} laeuft in {} Tag(en) ab (Zielpfad: {}).",
                        entry.adr_id,
                        entry.title_summary(),
                        due_str,
                        days_until_due,
                        target_desc
                    ));
                }
            }
        }
    }

    result
}

pub fn check_adr_deadlines_with_root(root: &Path) -> Result<(), String> {
    println!("=== Running xtask check-adr-deadlines ===");
    let decisions_path = root.join("DECISIONS.md");
    let decisions_content = fs::read_to_string(&decisions_path).map_err(|e| {
        format!(
            "DECISIONS.md ({}) nicht lesbar: {e}",
            decisions_path.display()
        )
    })?;

    let entries = parse_adr_deadlines(&decisions_content)?;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let deadline_res = check_adr_deadlines_at(&entries, &today, root);

    for w in &deadline_res.warnings {
        println!("{w}");
    }

    if !deadline_res.errors.is_empty() {
        for err in &deadline_res.errors {
            eprintln!("{err}");
        }
        return Err(format!(
            "ADR deadline check failed with {} error(s)",
            deadline_res.errors.len()
        ));
    }

    if deadline_res.warnings.is_empty() && deadline_res.errors.is_empty() {
        println!(
            "✅ Alle ADR-Deprecation-Fristen innerhalb des zulässigen Zeitfensters (überprüft: {} ADR(s)).",
            entries.len()
        );
    }

    Ok(())
}

pub fn check_adr_deadlines() -> Result<(), String> {
    let root = crate::find_root_dir();
    check_adr_deadlines_with_root(&root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_adr_deadlines() {
        let content = r#"
# ADR-077: Produktvision PyPI-Library Fokus und Tauri Deprecation

* **Status:** Akzeptiert
* **Datum:** 2026-09-08
* **Removal Deadline:** 2026-11-07
* **Target Path:** crates/memfuse-tauri
* **Kontext / Auslöser:** Zielarchitektur v8.0 §6
"#;
        let entries = parse_adr_deadlines(content).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].adr_id, "ADR-077");
        assert_eq!(
            entries[0].title,
            "Produktvision PyPI-Library Fokus und Tauri Deprecation"
        );
        assert_eq!(entries[0].deadline.as_deref(), Some("2026-11-07"));
        assert_eq!(
            entries[0].target_path.as_deref(),
            Some("crates/memfuse-tauri")
        );
    }

    #[test]
    fn test_adr_deadline_far_future_no_warning_no_error() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("crates/memfuse-tauri");
        fs::create_dir_all(&target).unwrap();

        let entries = vec![AdrDeadlineEntry {
            adr_id: "ADR-077".to_string(),
            title: "Tauri Deprecation".to_string(),
            status: "Akzeptiert".to_string(),
            deadline: Some("2026-11-07".to_string()),
            target_path: Some("crates/memfuse-tauri".to_string()),
        }];

        // Current date: 2026-09-09 (59 days left -> far future)
        let res = check_adr_deadlines_at(&entries, "2026-09-09", temp.path());
        assert!(res.warnings.is_empty());
        assert!(res.errors.is_empty());
    }

    #[test]
    fn test_adr_deadline_warning_window() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("crates/memfuse-tauri");
        fs::create_dir_all(&target).unwrap();

        let entries = vec![AdrDeadlineEntry {
            adr_id: "ADR-077".to_string(),
            title: "Tauri Deprecation".to_string(),
            status: "Akzeptiert".to_string(),
            deadline: Some("2026-11-07".to_string()),
            target_path: Some("crates/memfuse-tauri".to_string()),
        }];

        // Current date: 2026-10-30 (8 days left -> warning window)
        let res = check_adr_deadlines_at(&entries, "2026-10-30", temp.path());
        assert_eq!(res.warnings.len(), 1);
        assert!(res.errors.is_empty());
        assert!(res.warnings[0].contains("ADR-077"));
        assert!(res.warnings[0].contains("8 Tag(en)"));
    }

    #[test]
    fn test_adr_deadline_expired_hard_error_when_target_exists() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("crates/memfuse-tauri");
        fs::create_dir_all(&target).unwrap();

        let entries = vec![AdrDeadlineEntry {
            adr_id: "ADR-077".to_string(),
            title: "Tauri Deprecation".to_string(),
            status: "Akzeptiert".to_string(),
            deadline: Some("2026-11-07".to_string()),
            target_path: Some("crates/memfuse-tauri".to_string()),
        }];

        // Current date: 2026-11-08 (deadline expired, target exists)
        let res = check_adr_deadlines_at(&entries, "2026-11-08", temp.path());
        assert!(res.warnings.is_empty());
        assert_eq!(res.errors.len(), 1);
        assert!(res.errors[0].contains("ADR-FRIST ÜBERSCHRITTEN"));
        assert!(res.errors[0].contains("ADR-077"));
    }

    #[test]
    fn test_adr_deadline_expired_ok_when_target_removed() {
        let temp = tempdir().unwrap();
        // Target directory NOT created

        let entries = vec![AdrDeadlineEntry {
            adr_id: "ADR-077".to_string(),
            title: "Tauri Deprecation".to_string(),
            status: "Akzeptiert".to_string(),
            deadline: Some("2026-11-07".to_string()),
            target_path: Some("crates/memfuse-tauri".to_string()),
        }];

        // Current date: 2026-11-08 (deadline expired, but target removed -> OK)
        let res = check_adr_deadlines_at(&entries, "2026-11-08", temp.path());
        assert!(res.warnings.is_empty());
        assert!(res.errors.is_empty());
    }
}
