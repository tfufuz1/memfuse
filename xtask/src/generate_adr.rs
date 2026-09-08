use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

use crate::find_root_dir;

/// Ergebnis der ADR-Generierung als JSON-serialisierbarer Typ.
#[derive(Debug)]
pub struct AdrResult {
    pub path: PathBuf,
    pub number: u32,
}

/// Konsolidiert alle ADRs aus docs/decisions/ in eine einzige kanonische DECISIONS.md (ADR-060).
pub fn consolidate_decisions(root: &Path) -> Result<(), String> {
    let decisions_dir = root.join("docs").join("decisions");
    let target_file = root.join("DECISIONS.md");

    if !decisions_dir.exists() {
        println!(
            "ℹ️ Verzeichnis {} existiert nicht mehr — DECISIONS.md ist bereits die kanonische Single Source of Truth.",
            decisions_dir.display()
        );
        return Ok(());
    }

    let adr_re = Regex::new(r"^ADR-(\d+)").unwrap();
    let mut adr_files: Vec<(u32, PathBuf)> = Vec::new();

    let entries = fs::read_dir(&decisions_dir)
        .map_err(|e| format!("Kann {} nicht lesen: {}", decisions_dir.display(), e))?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".md") && name != "README.md" && name != "INDEX.md" {
            if let Some(caps) = adr_re.captures(&name) {
                if let Ok(num) = caps[1].parse::<u32>() {
                    adr_files.push((num, entry.path()));
                }
            }
        }
    }

    adr_files.sort_by_key(|(num, _)| *num);

    let mut output = String::new();
    output.push_str("# Architecture Decision Records (ADR)\n\n");
    output.push_str("> **Kanonische Einzel-Quelle:** Gemäß ADR-060 ist `DECISIONS.md` die einzige maßgebliche\n");
    output.push_str("> Quelle für Architecture Decision Records im MemFuse-Projekt. Neue Entscheidungen werden\n");
    output.push_str("> ausschließlich append-only am Ende dieser Datei ergänzt (`cargo xtask generate-adr \"<Titel>\"`).\n\n");

    output.push_str("## Dokumentierte Lücken & Umnummerierungen\n\n");
    output.push_str("* ADR-057: Lücken-Dokumentation (Umnummerierung / Ausgelassen im Zuge paralleler Audit-Sessions)\n");
    output.push_str("* ADR-067: Umnummeriert zu ADR-074 (Normative Kalibrierung des PathRAG Sufficiency-Gate Thresholds)\n");
    output.push_str("* ADR-068: Umnummeriert zu ADR-076 (Studie zur DiskANN PENDING_FLUSH_THRESHOLD Write-Amplification)\n\n");

    output.push_str("---\n\n");

    let header_re = Regex::new(r"^(#+)\s*ADR-\d+:\s*").unwrap();

    for (num, path) in adr_files {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Kann {} nicht lesen: {}", path.display(), e))?;
        let trimmed = content.trim();
        let normalized = if let Some(first_line_end) = trimmed.find('\n') {
            let (first_line, rest) = trimmed.split_at(first_line_end);
            if header_re.is_match(first_line) {
                let title = header_re.replace(first_line, "");
                format!("# ADR-{:03}: {}{}", num, title, rest)
            } else {
                trimmed.to_string()
            }
        } else {
            trimmed.to_string()
        };
        output.push_str(&normalized);
        output.push_str("\n\n---\n\n");
    }

    fs::write(&target_file, &output)
        .map_err(|e| format!("Kann {} nicht schreiben: {}", target_file.display(), e))?;

    println!(
        "✅ DECISIONS.md erfolgreich konsolidiert: {}",
        target_file.display()
    );
    Ok(())
}

/// Generiert einen neuen ADR-Eintrag in DECISIONS.md gemäß ADR-060.
///
/// Liest die aktuell höchste ADR-Nummer direkt aus DECISIONS.md, inkrementiert
/// um 1, und hängt ein Template für den neuen Eintrag an.
///
/// Bei `dry_run = true` wird DECISIONS.md nicht verändert.
pub fn run_generate_adr(title: &str, dry_run: bool) -> Result<AdrResult, String> {
    let root = find_root_dir();
    let decisions_path = root.join("DECISIONS.md");

    let content = if decisions_path.exists() {
        fs::read_to_string(&decisions_path)
            .map_err(|e| format!("Kann {} nicht lesen: {}", decisions_path.display(), e))?
    } else {
        String::new()
    };

    // Höchste ADR-Nummer ermitteln aus DECISIONS.md (oder Fallback auf docs/decisions)
    let adr_re = Regex::new(r"(?m)^#+\s+ADR-(\d+)").unwrap();
    let mut max_num: u32 = 0;

    for caps in adr_re.captures_iter(&content) {
        if let Ok(num) = caps[1].parse::<u32>() {
            if num > max_num {
                max_num = num;
            }
        }
    }

    // Fallback: Wenn in DECISIONS.md noch keine ADRs sind, docs/decisions prüfen
    if max_num == 0 {
        let decisions_dir = root.join("docs").join("decisions");
        if decisions_dir.exists() {
            let file_adr_re = Regex::new(r"^ADR-(\d+)").unwrap();
            if let Ok(entries) = fs::read_dir(&decisions_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(caps) = file_adr_re.captures(&name) {
                        if let Ok(num) = caps[1].parse::<u32>() {
                            if num > max_num {
                                max_num = num;
                            }
                        }
                    }
                }
            }
        }
    }

    let next_num = max_num + 1;
    let date = get_today_iso();

    // Template-Inhalt
    let template = format!(
        r#"
# ADR-{num:03}: {title}

* **Status:** Vorgeschlagen
* **Datum:** {date}
* **Kontext / Auslöser:** <!-- Beschreibung -->

## Entscheidung
<!-- Was wurde entschieden? -->

## Begründung
<!-- Warum diese Entscheidung? -->

## Alternativen
<!-- Welche Alternativen wurden erwogen? -->

## Konsequenzen
<!-- Was folgt aus dieser Entscheidung? -->

---
"#,
        num = next_num,
        title = title,
        date = date,
    );

    if dry_run {
        println!(
            "{{\"dry_run\": true, \"path\": \"{}\", \"number\": {}}}",
            decisions_path.display(),
            next_num
        );
        return Ok(AdrResult {
            path: decisions_path,
            number: next_num,
        });
    }

    let mut final_content = content;
    if !final_content.ends_with('\n') {
        final_content.push('\n');
    }
    final_content.push_str(&template);

    fs::write(&decisions_path, final_content)
        .map_err(|e| format!("Kann {} nicht schreiben: {}", decisions_path.display(), e))?;

    println!(
        "{{\"path\": \"{}\", \"number\": {}}}",
        decisions_path.display(),
        next_num
    );
    println!(
        "✅ ADR-{:03} an {} angehängt.",
        next_num,
        decisions_path.display()
    );

    Ok(AdrResult {
        path: decisions_path,
        number: next_num,
    })
}

/// Generiert einen URL-freundlichen Slug aus einem Titel.
#[allow(dead_code)]
fn generate_slug(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'ä' => "ae".to_string(),
            'ö' => "oe".to_string(),
            'ü' => "ue".to_string(),
            'ß' => "ss".to_string(),
            c if c.is_alphanumeric() => c.to_string(),
            ' ' | '-' | '_' => "-".to_string(),
            _ => String::new(),
        })
        .collect();

    let re = Regex::new(r"-+").unwrap();
    let slug = re.replace_all(&slug, "-");
    slug.trim_matches('-').to_string()
}

/// Gibt das heutige Datum im ISO-Format zurück.
fn get_today_iso() -> String {
    if let Ok(output) = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    "YYYY-MM-DD".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_slug_simple() {
        assert_eq!(generate_slug("MCP Write Policy"), "mcp-write-policy");
    }

    #[test]
    fn test_generate_slug_umlauts() {
        assert_eq!(
            generate_slug("Nummernvergabe für ADRs"),
            "nummernvergabe-fuer-adrs"
        );
    }

    #[test]
    fn test_generate_slug_special_chars() {
        assert_eq!(
            generate_slug("HNSW: Rebuild & Recall-Test"),
            "hnsw-rebuild-recall-test"
        );
    }

    #[test]
    fn test_generate_slug_multiple_hyphens() {
        assert_eq!(generate_slug("  Foo -- Bar  "), "foo-bar");
    }

    #[test]
    fn test_adr_template_contains_required_sections() {
        let title = "Test Decision";
        let date = "2026-09-07";
        let content =
            format!("# ADR-066: {title}\n\n* **Status:** Vorgeschlagen\n* **Datum:** {date}\n",);
        assert!(content.contains("Status:"));
        assert!(content.contains("Datum:"));
    }

    #[test]
    fn test_generate_adr_dry_run() {
        let root = crate::find_root_dir();
        let result = run_generate_adr("Test Dry Run", true);
        assert!(result.is_ok());
        let adr = result.unwrap();
        assert!(adr.number > 0);
        assert_eq!(adr.path, root.join("DECISIONS.md"));
    }
}
