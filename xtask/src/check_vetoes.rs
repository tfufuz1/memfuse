use std::fs;
use std::process::Command;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct VetoEntry {
    pub feature_id: String,
    pub status: String,
    pub keywords: Vec<String>,
}

pub fn parse_vetoes(content: &str) -> Result<Vec<VetoEntry>, String> {
    let mut entries = Vec::new();
    let blocks = content.split("## VETO-");

    for block in blocks.skip(1) {
        let mut feature_id = String::new();
        let mut status = String::new();
        let mut keywords = Vec::new();

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("feature_id:") {
                feature_id = trimmed.trim_start_matches("feature_id:").trim().to_string();
            } else if trimmed.starts_with("status:") {
                status = trimmed.trim_start_matches("status:").trim().to_string();
            } else if trimmed.starts_with("keywords:") {
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
            }
        }

        if !feature_id.is_empty() && !status.is_empty() {
            entries.push(VetoEntry {
                feature_id,
                status,
                keywords,
            });
        }
    }

    Ok(entries)
}

pub fn check_vetoes() -> Result<(), String> {
    println!("=== Running xtask check-vetoes ===");
    let vetoes_content =
        fs::read_to_string("VETOES.md").map_err(|e| format!("VETOES.md nicht lesbar: {e}"))?;

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

    if !warnings.is_empty() {
        for w in &warnings {
            println!("{w}");
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
}
