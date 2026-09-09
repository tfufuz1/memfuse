// MemFuse — Feature-Veto-Register & CI Gate
//
// Modul zur Überprüfung von `VETOES.md` im CI-Workflow.
//
// Prüflogik:
// 1. Permanent Rejected: Durchsucht die letzten 50 Git-Commits nach Schlüsselwörtern abgelehnter Features.
// 2. Conditionally Accepted: Überwacht `conditional_review_due` Fristen.
//    - Frist in der Vergangenheit (< 0 Tage): Harter CI-Fehler (Exit-Code != 0) mit Referenz auf `adr_ref`.
//    - Frist innerhalb von CONDITIONAL_REVIEW_WARNING_THRESHOLD_DAYS (7 Tage): Warnung zur rechtzeitigen Review.
//    - Frist weit in der Zukunft (> 7 Tage): Kein Hinweis.

use chrono::NaiveDate;
use std::fs;
use std::process::Command;

pub const CONDITIONAL_REVIEW_WARNING_THRESHOLD_DAYS: i64 = 14;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct VetoEntry {
    pub feature_id: String,
    pub status: String,
    pub keywords: Vec<String>,
    pub reason: Option<String>,
    pub adr_ref: Option<String>,
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
        let mut adr_ref = None;
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
            } else if trimmed.starts_with("review_date:")
                || trimmed.starts_with("conditional_review_due:")
            {
                parsing_reason = false;
                let due = if trimmed.starts_with("review_date:") {
                    trimmed
                        .trim_start_matches("review_date:")
                        .trim()
                        .to_string()
                } else {
                    trimmed
                        .trim_start_matches("conditional_review_due:")
                        .trim()
                        .to_string()
                };
                if !due.is_empty() {
                    conditional_review_due = Some(due);
                }
            } else if trimmed.starts_with("adr_ref:") {
                parsing_reason = false;
                let ar = trimmed.trim_start_matches("adr_ref:").trim().to_string();
                if !ar.is_empty() && ar != "null" {
                    adr_ref = Some(ar);
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
            } else if trimmed.starts_with("scope_note:") || trimmed.starts_with("last_verified:") {
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
                adr_ref,
                conditional_review_due,
            });
        }
    }

    Ok(entries)
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct DeadlineCheckResult {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub fn check_conditional_review_deadlines_at(
    entries: &[VetoEntry],
    today_str: &str,
) -> DeadlineCheckResult {
    let mut result = DeadlineCheckResult::default();
    let today = match NaiveDate::parse_from_str(today_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return result,
    };

    for entry in entries {
        if entry.status == "conditionally_accepted" {
            if let Some(due_str) = &entry.conditional_review_due {
                if let Ok(due_date) = NaiveDate::parse_from_str(due_str, "%Y-%m-%d") {
                    let days_until_due = (due_date - today).num_days();
                    let adr = entry.adr_ref.as_deref().unwrap_or("keine ADR angegeben");

                    if days_until_due < 0 {
                        result.errors.push(format!(
                            "❌ VETO-FRIST ÜBERSCHRITTEN: {} — Wiedervorlage war am {}, bitte ADR mit Entscheidung erstellen oder Frist explizit per neuem ADR verlängern (adr_ref: {}).",
                            entry.feature_id,
                            due_str,
                            adr
                        ));
                    } else if days_until_due <= CONDITIONAL_REVIEW_WARNING_THRESHOLD_DAYS {
                        result.warnings.push(format!(
                            "⚠️ WARNUNG: VETO {} ('{}'): Review-Frist {} laeuft in {} Tag(en) ab — rechtzeitige Review erforderlich (adr_ref: {}).",
                            entry.feature_id,
                            entry.reason_summary(),
                            due_str,
                            days_until_due,
                            adr
                        ));
                    }
                }
            }
        }
    }

    result
}

pub fn check_conditional_review_deadlines(entries: &[VetoEntry]) -> DeadlineCheckResult {
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

    let deadline_res = check_conditional_review_deadlines(&entries);
    warnings.extend(deadline_res.warnings);

    if !warnings.is_empty() {
        for w in &warnings {
            eprintln!("{w}");
        }
    } else if deadline_res.errors.is_empty() {
        println!("✅ Keine Veto-Keyword-Treffer in den letzten 50 Commits.");
    }

    if !deadline_res.errors.is_empty() {
        for err in &deadline_res.errors {
            eprintln!("{err}");
        }
        return Err(format!(
            "Veto conditional review check failed with {} error(s)",
            deadline_res.errors.len()
        ));
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
    fn test_conditional_review_deadline_expired_hard_error() {
        let entries = vec![VetoEntry {
            feature_id: "F-02".to_string(),
            status: "conditionally_accepted".to_string(),
            keywords: vec![],
            reason: Some("Test reason".to_string()),
            adr_ref: Some("docs/decisions/ADR-0XX-test.md".to_string()),
            conditional_review_due: Some("2026-10-07".to_string()),
        }];

        let res = check_conditional_review_deadlines_at(&entries, "2026-10-08");
        assert!(res.warnings.is_empty());
        assert_eq!(res.errors.len(), 1);
        assert_eq!(
            res.errors[0],
            "❌ VETO-FRIST ÜBERSCHRITTEN: F-02 — Wiedervorlage war am 2026-10-07, bitte ADR mit Entscheidung erstellen oder Frist explizit per neuem ADR verlängern (adr_ref: docs/decisions/ADR-0XX-test.md)."
        );
    }

    #[test]
    fn test_conditional_review_deadline_warning_within_threshold() {
        let entries = vec![VetoEntry {
            feature_id: "F-02".to_string(),
            status: "conditionally_accepted".to_string(),
            keywords: vec![],
            reason: Some("Test reason".to_string()),
            adr_ref: Some("docs/decisions/ADR-0XX-test.md".to_string()),
            conditional_review_due: Some("2026-10-07".to_string()),
        }];

        // 10 days before due date (within 14-day threshold)
        let res = check_conditional_review_deadlines_at(&entries, "2026-09-27");
        assert_eq!(res.warnings.len(), 1);
        assert!(res.errors.is_empty());
        assert!(res.warnings[0].contains("F-02"));
        assert!(res.warnings[0].contains("2026-10-07"));
        assert!(res.warnings[0].contains("10 Tag(en)"));
    }

    #[test]
    fn test_conditional_review_deadline_far_future_no_warning_no_error() {
        let entries = vec![VetoEntry {
            feature_id: "F-02".to_string(),
            status: "conditionally_accepted".to_string(),
            keywords: vec![],
            reason: Some("Test reason".to_string()),
            adr_ref: Some("docs/decisions/ADR-0XX-test.md".to_string()),
            conditional_review_due: Some("2026-10-07".to_string()),
        }];

        // 22 days before due date (> 14 days)
        let res = check_conditional_review_deadlines_at(&entries, "2026-09-15");
        assert!(res.warnings.is_empty());
        assert!(res.errors.is_empty());
    }

    #[test]
    fn test_review_date_field_parsing_and_deadline_check() {
        let content = r#"
## VETO-OP3
feature_id: OP-03
status: conditionally_accepted
review_date: 2026-10-07
adr_ref: DECISIONS.md#adr-077
reason: >
  Test OP-03 review date
"#;
        let entries = parse_vetoes(content).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].conditional_review_due.as_deref(),
            Some("2026-10-07")
        );

        // Expired check
        let expired_res = check_conditional_review_deadlines_at(&entries, "2026-10-10");
        assert_eq!(expired_res.errors.len(), 1);
        assert_eq!(
            expired_res.errors[0],
            "❌ VETO-FRIST ÜBERSCHRITTEN: OP-03 — Wiedervorlage war am 2026-10-07, bitte ADR mit Entscheidung erstellen oder Frist explizit per neuem ADR verlängern (adr_ref: DECISIONS.md#adr-077)."
        );

        // Warning check (within 14 days)
        let warning_res = check_conditional_review_deadlines_at(&entries, "2026-09-25");
        assert_eq!(warning_res.warnings.len(), 1);
        assert!(warning_res.warnings[0].contains("12 Tag(en)"));

        // Far future check
        let ok_res = check_conditional_review_deadlines_at(&entries, "2026-08-01");
        assert!(ok_res.warnings.is_empty());
        assert!(ok_res.errors.is_empty());
    }

    #[test]
    fn test_no_warning_or_error_for_permanent_rejected_entries() {
        let entries = vec![VetoEntry {
            feature_id: "F-10".to_string(),
            status: "permanent_rejected".to_string(),
            keywords: vec![],
            reason: Some("Permanent rejected reason".to_string()),
            adr_ref: None,
            conditional_review_due: Some("2026-09-01".to_string()),
        }];

        let res = check_conditional_review_deadlines_at(&entries, "2026-10-08");
        assert!(res.warnings.is_empty());
        assert!(res.errors.is_empty());
    }

    #[test]
    fn test_synthetic_vetoes_fixture_all_three_codepaths() {
        let fixture = r#"
## VETO-F01
feature_id: F-01
status: conditionally_accepted
conditional_review_due: 2026-08-01
adr_ref: docs/decisions/ADR-001.md
reason: >
  Expired entry

## VETO-F02
feature_id: F-02
status: conditionally_accepted
conditional_review_due: 2026-08-30
adr_ref: docs/decisions/ADR-002.md
reason: >
  Warning window entry

## VETO-F03
feature_id: F-03
status: conditionally_accepted
conditional_review_due: 2026-10-15
adr_ref: docs/decisions/ADR-003.md
reason: >
  Far future entry
"#;
        let entries = parse_vetoes(fixture).unwrap();
        assert_eq!(entries.len(), 3);

        // Assume build date is 2026-08-25:
        // F-01 (due 2026-08-01): 24 days overdue -> HARTER FEHLER
        // F-02 (due 2026-08-30): 5 days left -> WARNUNG
        // F-03 (due 2026-10-15): 51 days left -> KEIN HINWEIS
        let res = check_conditional_review_deadlines_at(&entries, "2026-08-25");

        assert_eq!(res.errors.len(), 1);
        assert!(res.errors[0].contains("F-01"));
        assert!(res.errors[0].contains("2026-08-01"));

        assert_eq!(res.warnings.len(), 1);
        assert!(res.warnings[0].contains("F-02"));
        assert!(res.warnings[0].contains("2026-08-30"));
        assert!(res.warnings[0].contains("5 Tag(en)"));
    }

    #[test]
    fn test_check_vetoes_against_current_repo_state() {
        let result = check_vetoes();
        assert!(result.is_ok());
    }
}
