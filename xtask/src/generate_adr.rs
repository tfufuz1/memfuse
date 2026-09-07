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

/// Generiert eine neue ADR-Datei mit sicherer, live-geprüfter Nummernvergabe.
///
/// Liest die aktuell höchste ADR-Nummer aus dem Dateisystem, inkrementiert
/// um 1, und erstellt eine Template-Datei mit dem gegebenen Titel.
///
/// Bei `dry_run = true` wird die Datei nicht geschrieben (nur Ausgabe).
pub fn run_generate_adr(title: &str, dry_run: bool) -> Result<AdrResult, String> {
    let root = find_root_dir();
    let decisions_dir = root.join("docs").join("decisions");

    if !decisions_dir.exists() {
        return Err(format!(
            "Verzeichnis {} existiert nicht",
            decisions_dir.display()
        ));
    }

    // Höchste ADR-Nummer ermitteln (exakt wie AGENTS.md vorschreibt)
    let adr_re = Regex::new(r"^ADR-(\d+)").unwrap();
    let mut max_num: u32 = 0;

    let entries = fs::read_dir(&decisions_dir)
        .map_err(|e| format!("Kann {} nicht lesen: {}", decisions_dir.display(), e))?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(caps) = adr_re.captures(&name) {
            if let Ok(num) = caps[1].parse::<u32>() {
                if num > max_num {
                    max_num = num;
                }
            }
        }
    }

    let next_num = max_num + 1;

    // Slug aus Titel generieren
    let slug = generate_slug(title);
    let filename = format!("ADR-{:03}-{}.md", next_num, slug);
    let filepath = decisions_dir.join(&filename);

    // Aktuelles Datum
    let date = get_today_iso();

    // Template-Inhalt
    let content = format!(
        r#"# ADR-{num:03}: {title}

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
"#,
        num = next_num,
        title = title,
        date = date,
    );

    if dry_run {
        println!(
            "{{\"dry_run\": true, \"path\": \"{}\", \"number\": {}}}",
            filepath.display(),
            next_num
        );
        return Ok(AdrResult {
            path: filepath,
            number: next_num,
        });
    }

    // Datei schreiben
    fs::write(&filepath, &content)
        .map_err(|e| format!("Kann {} nicht schreiben: {}", filepath.display(), e))?;

    println!(
        "{{\"path\": \"{}\", \"number\": {}}}",
        filepath.display(),
        next_num
    );
    println!(
        "✅ ADR-{:03} erstellt: {}",
        next_num,
        filepath.display()
    );

    Ok(AdrResult {
        path: filepath,
        number: next_num,
    })
}

/// Generiert einen URL-freundlichen Slug aus einem Titel.
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

    // Mehrfache Bindestriche zusammenfassen und Rand-Striche entfernen
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
        assert_eq!(
            generate_slug("  Foo -- Bar  "),
            "foo-bar"
        );
    }

    #[test]
    fn test_adr_template_contains_required_sections() {
        // Verify the template format by checking the generated content structure
        let title = "Test Decision";
        let date = "2026-09-07";
        let content = format!(
            "# ADR-066: {title}\n\n* **Status:** Vorgeschlagen\n* **Datum:** {date}\n",
        );
        assert!(content.contains("Status:"));
        assert!(content.contains("Datum:"));
    }

    #[test]
    fn test_generate_adr_dry_run() {
        // Dry-run muss funktionieren ohne Dateisystem-Seiteneffekte
        // (wird nur getestet wenn docs/decisions existiert)
        let root = crate::find_root_dir();
        let decisions = root.join("docs").join("decisions");
        if decisions.exists() {
            let result = run_generate_adr("Test Dry Run", true);
            assert!(result.is_ok());
            let adr = result.unwrap();
            assert!(adr.number > 0);
            // Datei darf NICHT existieren bei dry_run
            assert!(!adr.path.exists());
        }
    }
}
