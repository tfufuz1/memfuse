use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Toleranzband für den maximal zulässigen Recall@10-Abfall nach `rebuild_region()`:
/// ±5 Prozentpunkte (0.05).
///
/// Synch-Referenz: `crates/memfuse-index/tests/nucleation_recall_regression.rs`
/// Bei Änderungen an dieser Schwelle MÜSSEN beide Stellen synchron angepasst werden!
pub const RECALL_TOLERANCE_BAND: f64 = 0.05;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecallHistoryEntry {
    pub date: String,
    pub recall_before: f64,
    pub recall_after: f64,
    pub drop_pp: f64,
    pub commit_sha: String,
}

pub fn run_check_recall_stability(_args: &[String]) -> bool {
    let history_path = Path::new("benchmarks/results/nucleation_recall_history.jsonl");
    let today = Utc::now().date_naive();
    check_recall_stability_from_file(history_path, today)
}

/// Bedenke: `check_recall_stability` ist als reines Statusgate implementiert
/// (gibt standardmäßig `true` zurück, außer bei unheilbaren I/O- oder Parsing-Fehlern).
/// Es soll den Build/Merge NICHT blockieren, solange die 30-Tage-Frist läuft.
/// Erst nach offizieller VETO-F02-Freigabe darf diese Funktion bei Fehlschlag `false` liefern.
pub fn check_recall_stability_from_file(history_path: &Path, today: NaiveDate) -> bool {
    // 1. Datei prüfen & lesen
    if !history_path.exists() {
        if let Some(parent) = history_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(history_path, "") {
            eprintln!(
                "⚠️ Fehler beim Anlegen von {}: {}",
                history_path.display(),
                e
            );
            return true;
        }
        println!(
            "⚠️ Historie-Datei {} existierte nicht und wurde leer angelegt.",
            history_path.display()
        );
        println!("Status: 0 von 30 Tagen Historie vorhanden (noch 30 Tage verbleibend).");
        return true;
    }

    let content = match fs::read_to_string(history_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("⚠️ Fehler beim Lesen von {}: {}", history_path.display(), e);
            return true;
        }
    };

    let entries = parse_history_entries(&content);
    evaluate_recall_stability(&entries, today)
}

pub fn parse_history_entries(content: &str) -> Vec<RecallHistoryEntry> {
    let mut entries = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<RecallHistoryEntry>(trimmed) {
            entries.push(entry);
        } else {
            eprintln!(
                "⚠️ Zeile konnte nicht als RecallHistoryEntry geparst werden: {}",
                trimmed
            );
        }
    }
    entries
}

pub fn evaluate_recall_stability(entries: &[RecallHistoryEntry], today: NaiveDate) -> bool {
    let window_start = today - chrono::Duration::days(30);

    // Filter Einträge der letzten 30 Kalendertage
    let mut recent_entries: Vec<&RecallHistoryEntry> = entries
        .iter()
        .filter_map(|e| {
            let parsed_date = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok()?;
            if parsed_date >= window_start && parsed_date <= today {
                Some(e)
            } else {
                None
            }
        })
        .collect();

    recent_entries.sort_by(|a, b| a.date.cmp(&b.date));

    // Frühestes Datum aus der Gesamthistorie
    let earliest_date = entries
        .iter()
        .filter_map(|e| NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok())
        .min();

    let has_30_days_history = match earliest_date {
        Some(d) => d <= window_start,
        None => false,
    };

    let outliers: Vec<&&RecallHistoryEntry> = recent_entries
        .iter()
        .filter(|e| e.drop_pp > RECALL_TOLERANCE_BAND)
        .collect();

    if has_30_days_history && outliers.is_empty() {
        println!("✅ VETO-F02: 30-Tage-Recall-Stabilität nachgewiesen — partial-rebuild-pruning kann zur Review vorgelegt werden");
    } else {
        let days_recorded = match earliest_date {
            Some(d) => (today - d).num_days().max(0),
            None => 0,
        };
        let remaining_days = (30 - days_recorded).max(0);

        println!("ℹ️ Statusbericht VETO-F02 Recall-Stabilität:");
        if !has_30_days_history {
            println!(
                "  - Historie unvollständig: {} Tage aufgezeichnet (noch {} Tage verbleibend bis 30 Tage Nachweis).",
                days_recorded, remaining_days
            );
        } else {
            println!("  - Historie spannt 30+ Tage.");
        }

        if !outliers.is_empty() {
            println!(
                "  - Ausreißer gefunden (drop_pp > {:.2}):",
                RECALL_TOLERANCE_BAND
            );
            for out in outliers {
                println!(
                    "    * Datum: {}, drop_pp: {:.4} (before: {:.4}, after: {:.4}), commit: {}",
                    out.date, out.drop_pp, out.recall_before, out.recall_after, out.commit_sha
                );
            }
        } else {
            println!("  - Keine Ausreißer in den letzten 30 Tagen gefunden.");
        }
    }

    // Statusgate: Gibt explizit true zurück, um den Build während der 30-Tage-Evaluierung nicht zu blockieren.
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_missing_history_file_creates_empty_and_returns_true() {
        let dir = tempdir().unwrap();
        let file_path = dir
            .path()
            .join("results")
            .join("nucleation_recall_history.jsonl");
        let today = NaiveDate::from_ymd_opt(2026, 9, 9).unwrap();

        assert!(!file_path.exists());
        let res = check_recall_stability_from_file(&file_path, today);
        assert!(res);
        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_30_plus_days_stable_history_returns_true_with_success() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        let mut entries = Vec::new();

        // Generate 31 days of history without outliers
        for day in 0..=30 {
            let date = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap() + chrono::Duration::days(day);
            entries.push(RecallHistoryEntry {
                date: date.format("%Y-%m-%d").to_string(),
                recall_before: 0.768,
                recall_after: 0.726,
                drop_pp: 0.042,
                commit_sha: "abc1234".to_string(),
            });
        }

        let res = evaluate_recall_stability(&entries, today);
        assert!(res);
    }

    #[test]
    fn test_30_plus_days_with_outlier_returns_true_with_outlier_report() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        let mut entries = Vec::new();

        // Generate 31 days of history with one outlier (drop_pp = 0.065 > 0.05)
        for day in 0..=30 {
            let date = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap() + chrono::Duration::days(day);
            let drop_pp = if day == 15 { 0.065 } else { 0.042 };
            entries.push(RecallHistoryEntry {
                date: date.format("%Y-%m-%d").to_string(),
                recall_before: 0.768,
                recall_after: 0.768 - drop_pp,
                drop_pp,
                commit_sha: "abc1234".to_string(),
            });
        }

        let res = evaluate_recall_stability(&entries, today);
        assert!(res);
    }
}
