use std::fs;
use std::process::Command;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct VetoEntry {
    pub feature_id: String,
    pub status: String,
    pub keywords: Vec<String>,
    pub reason: Option<String>,
    pub conditional_review_due: Option<String>,
}

impl VetoEntry {
    pub fn reason_summary(&self) -> &str {
        match &self.reason {
            Some(r) => {
                let trimmed = r.trim();
                if trimmed.is_empty() {
                    &self.feature_id
                } else {
                    trimmed.lines().next().unwrap_or(&self.feature_id).trim()
                }
            }
            None => &self.feature_id,
        }
    }
}

pub fn parse_vetoes(content: &str) -> Result<Vec<VetoEntry>, String> {
    let mut entries = Vec::new();
    let blocks = content.split("## VETO-");

    for block in blocks.skip(1) {
        let mut feature_id = String::new();
        let mut status = String::new();
        let mut keywords = Vec::new();
        let mut conditional_review_due = None;
        let mut reason_lines = Vec::new();
        let mut parsing_reason = false;

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("feature_id:") {
                parsing_reason = false;
                feature_id = trimmed.trim_start_matches("feature_id:").trim().to_string();
            } else if trimmed.starts_with("status:") {
                parsing_reason = false;
                status = trimmed.trim_start_matches("status:").trim().to_string();
            } else if trimmed.starts_with("conditional_review_due:") {
                parsing_reason = false;
                let due = trimmed
                    .trim_start_matches("conditional_review_due:")
                    .trim()
                    .to_string();
                if !due.is_empty() {
                    conditional_review_due = Some(due);
                }
            } else if trimmed.starts_with("keywords:") {
                parsing_reason = false;
                let kw_str = trimmed.trim_start_matches("keywords:").trim();
                if kw_str.starts_with('[') && kw_str.ends_with(']') {
                    let inner = &kw_str[1..kw_str.len() - 1];
                    for item in inner.split(',') {
                        let cleaned = item.trim().trim_matches('"').trim_matches('\'').to_string();
                        if !cleaned.is_empty() {
                            keywords.push(cleaned);
                        }
                    }
                }
            } else if trimmed.starts_with("reason:") {
                parsing_reason = true;
                let rest = trimmed.trim_start_matches("reason:").trim();
                if !rest.is_empty() && rest != ">" {
                    reason_lines.push(rest.to_string());
                }
            } else if trimmed.starts_with("scope_note:")
                || trimmed.starts_with("adr_ref:")
                || trimmed.starts_with("last_verified:")
            {
                parsing_reason = false;
            } else if parsing_reason && !trimmed.is_empty() {
                reason_lines.push(trimmed.to_string());
            }
        }

        let reason = if reason_lines.is_empty() {
            None
        } else {
            Some(reason_lines.join(" "))
        };

        if !feature_id.is_empty() && !status.is_empty() {
            entries.push(VetoEntry {
                feature_id,
                status,
                keywords,
                reason,
                conditional_review_due,
            });
        }
    }

    Ok(entries)
}

pub fn check_conditional_review_deadlines_at(entries: &[VetoEntry], today: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    for entry in entries {
        if entry.status == "conditionally_accepted" {
            if let Some(due) = &entry.conditional_review_due {
                if today >= due.as_str() {
                    warnings.push(format!(
                        "⚠️  VETO {} ('{}'): Review-Frist {} erreicht/überschritten seit {} — \
                         bewusste Neubewertung erforderlich (endgültig freigeben, \
                         verlängern mit neuem Datum, oder auf permanent_rejected setzen).",
                        entry.feature_id,
                        entry.reason_summary(),
                        due,
                        today
                    ));
                }
            }
        }
    }
    warnings
}

pub fn check_conditional_review_deadlines(entries: &[VetoEntry]) -> Vec<String> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    check_conditional_review_deadlines_at(entries, &today)
}

pub fn check_vetoes() -> Result<(), String> {
    println!("=== Running xtask check-vetoes ===");
    let root = crate::find_root_dir();
    let vetoes_path = root.join("VETOES.md");
    let vetoes_content = fs::read_to_string(&vetoes_path)
        .map_err(|e| format!("VETOES.md ({}) nicht lesbar: {e}", vetoes_path.display()))?;

    let entries = parse_vetoes(&vetoes_content)?;

    let log_output = Command::new("git")
        .args(["log", "--oneline", "-50"])
        .output()
        .map_err(|e| format!("git log fehlgeschlagen: {e}"))?;

    let commit_log = String::from_utf8_lossy(&log_output.stdout).to_lowercase();

    let mut warnings = Vec::new();
    for entry in entries.iter().filter(|e| e.status == "permanent_rejected") {
        for keyword in &entry.keywords {
            if commit_log.contains(&keyword.to_lowercase()) {
                warnings.push(format!(
                    "⚠️  Möglicher VETO-Treffer: {} (Keyword: '{}'). Siehe VETOES.md. \
                     Falls legitim: PR-Beschreibung mit 'VETO-CHECK-OK: <Grund>' versehen.",
                    entry.feature_id, keyword
                ));
            }
        }
    }

    let deadline_warnings = check_conditional_review_deadlines(&entries);
    warnings.extend(deadline_warnings);

    if !warnings.is_empty() {
        for w in &warnings {
            eprintln!("{w}");
        }
        // Kein Err() -- bewusst nur Warnung, kein Hard-Fail (False-Positive-Risiko)
    } else {
        println!("✅ Keine Veto-Keyword-Treffer in den letzten 50 Commits.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vetoes() {
        let content = r#"
# MemFuse — Feature-Veto-Register

## VETO-F02

feature_id: F-02
status: conditionally_accepted
conditional_review_due: 2026-10-07
keywords: ["partial hnsw rebuild", "nucleation", "rebuild_region", "F-02"]
reason: >
  Reason text

## VETO-F10

feature_id: F-10
status: permanent_rejected
keywords: ["cross-tenant", "osmotic knowledge exchange", "tenant knowledge sharing", "F-10"]
reason: >
  Reason text
"#;
        let entries = parse_vetoes(content).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].feature_id, "F-02");
        assert_eq!(entries[0].status, "conditionally_accepted");
        assert_eq!(
            entries[0].conditional_review_due.as_deref(),
            Some("2026-10-07")
        );
        assert_eq!(
            entries[0].keywords,
            vec![
                "partial hnsw rebuild",
                "nucleation",
                "rebuild_region",
                "F-02"
            ]
        );

        assert_eq!(entries[1].feature_id, "F-10");
        assert_eq!(entries[1].status, "permanent_rejected");
        assert_eq!(entries[1].conditional_review_due, None);
        assert_eq!(
            entries[1].keywords,
            vec![
                "cross-tenant",
                "osmotic knowledge exchange",
                "tenant knowledge sharing",
                "F-10"
            ]
        );
    }

    #[test]
    fn test_conditional_review_deadline_warning_when_overdue() {
        let entries = vec![VetoEntry {
            feature_id: "F-02".to_string(),
            status: "conditionally_accepted".to_string(),
            keywords: vec![],
            reason: Some("Test reason".to_string()),
            conditional_review_due: Some("2026-10-07".to_string()),
        }];

        let warnings = check_conditional_review_deadlines_at(&entries, "2026-10-08");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("F-02"));
        assert!(warnings[0].contains("2026-10-07"));
        assert!(warnings[0].contains("2026-10-08"));
    }

    #[test]
    fn test_no_warning_when_deadline_not_yet_reached() {
        let entries = vec![VetoEntry {
            feature_id: "F-02".to_string(),
            status: "conditionally_accepted".to_string(),
            keywords: vec![],
            reason: Some("Test reason".to_string()),
            conditional_review_due: Some("2026-10-07".to_string()),
        }];

        let warnings = check_conditional_review_deadlines_at(&entries, "2026-09-15");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_no_warning_for_permanent_rejected_entries() {
        let entries = vec![VetoEntry {
            feature_id: "F-10".to_string(),
            status: "permanent_rejected".to_string(),
            keywords: vec![],
            reason: Some("Permanent rejected reason".to_string()),
            conditional_review_due: Some("2026-09-01".to_string()),
        }];

        let warnings = check_conditional_review_deadlines_at(&entries, "2026-10-08");
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_check_vetoes_exit_code_zero_despite_overdue_warning() {
        // Calling check_vetoes() against VETOES.md should return Ok(()) regardless of warnings.
        let result = check_vetoes();
        assert!(result.is_ok());
    }
}
