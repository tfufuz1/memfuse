use std::fs;
use std::path::Path;

use crate::find_root_dir;

/// Bootstrappt ein TDD-Harness für einen Audit-Fix.
///
/// Analysiert den gegebenen Commit-Hash, ermittelt die betroffenen Crates
/// und erstellt isolierte Test-Dateien gemäß AUDIT_INTAKE_PROTOCOL.md.
pub fn run_init_audit_fix(commit_hash: &str) -> Result<(), String> {
    let root = find_root_dir();

    // Validiere Commit-Hash (mindestens 7 Hex-Zeichen)
    if commit_hash.len() < 7
        || !commit_hash
            .chars()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Err(format!(
            "Ungültiger Commit-Hash: '{}' — mindestens 7 Hex-Zeichen erwartet",
            commit_hash
        ));
    }

    let short_hash = &commit_hash[..7.min(commit_hash.len())];

    // Betroffene Dateien via git show ermitteln
    let output = std::process::Command::new("git")
        .args(["show", "--stat", "--format=", commit_hash])
        .current_dir(&root)
        .output()
        .map_err(|e| format!("git show fehlgeschlagen: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Commit '{}' nicht gefunden. git show Fehler: {}",
            commit_hash,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stat_output = String::from_utf8_lossy(&output.stdout);
    let changed_files: Vec<&str> = stat_output
        .lines()
        .filter(|line| line.contains('|'))
        .filter_map(|line| line.split('|').next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if changed_files.is_empty() {
        return Err(format!(
            "Keine Dateien in Commit '{}' gefunden",
            commit_hash
        ));
    }

    // Betroffene Crates identifizieren
    let mut affected_crates: Vec<String> = changed_files
        .iter()
        .filter(|f| f.starts_with("crates/"))
        .filter_map(|f| f.split('/').nth(1))
        .map(|s| s.to_string())
        .collect();
    affected_crates.sort();
    affected_crates.dedup();

    if affected_crates.is_empty() {
        // Dateien liegen nicht unter crates/ — Fallback auf Root-Ebene
        println!("⚠️  Commit betrifft keine Crate-Dateien. Erstelle Test im ersten betroffenen Pfad.");
        println!("Betroffene Dateien:");
        for f in &changed_files {
            println!("  {}", f);
        }
        return Ok(());
    }

    println!(
        "=== xtask init-audit-fix {} ===",
        short_hash
    );
    println!("Betroffene Crates: {}", affected_crates.join(", "));
    println!();

    // Für jedes betroffene Crate eine Test-Datei erstellen
    for krate in &affected_crates {
        let crate_dir = root.join("crates").join(krate);
        if !crate_dir.exists() {
            eprintln!("⚠️  Crate-Verzeichnis {} existiert nicht — überspringe", krate);
            continue;
        }

        let tests_dir = crate_dir.join("tests");
        if !tests_dir.exists() {
            fs::create_dir_all(&tests_dir)
                .map_err(|e| format!("Kann {} nicht erstellen: {}", tests_dir.display(), e))?;
        }

        let test_filename = format!("audit_fix_{}.rs", short_hash);
        let test_path = tests_dir.join(&test_filename);

        if test_path.exists() {
            println!("⚠️  {} existiert bereits — überspringe", test_path.display());
            continue;
        }

        // Dateiliste für dieses Crate filtern
        let crate_prefix = format!("crates/{}/", krate);
        let crate_files: Vec<&&str> = changed_files
            .iter()
            .filter(|f| f.starts_with(&crate_prefix))
            .collect();

        let files_comment: String = crate_files
            .iter()
            .map(|f| format!("//!   - {}", f))
            .collect::<Vec<_>>()
            .join("\n");

        let test_content = format!(
            r#"//! Audit-Fix Test für Commit {full_hash}
//! Erstellt via: cargo xtask init-audit-fix {full_hash}
//! Protokoll: .jules/AUDIT_INTAKE_PROTOCOL.md
//!
//! SCHRITT 1: Schreibe einen fehlschlagenden Test der das Problem reproduziert
//! SCHRITT 2: Verifiziere mit `cargo test -p {krate} --test audit_fix_{short}`
//! SCHRITT 3: Implementiere den Fix
//! SCHRITT 4: Test muss grün sein + `just check`
//!
//! Betroffene Dateien:
{files}

#[test]
fn audit_fix_{short}_reproduce() {{
    // Reproduziere den Fehler aus Commit {full_hash}
    //
    // Leitfaden (aus .jules/AUDIT_INTAKE_PROTOCOL.md):
    // 1. Datei & Zeile im AKTUELLEN Code öffnen
    // 2. Problem im AKTUELLEN Stand verifizieren
    // 3. Falls ENTKRÄFTET: als [ENTKRÄFTET] markieren mit Begründung
    // 4. Falls AKTIV: Diesen Test so schreiben, dass er FEHLSCHLÄGT
    //
    todo!("Schreibe erst den roten Test, dann den Fix")
}}
"#,
            full_hash = commit_hash,
            krate = krate,
            short = short_hash,
            files = files_comment,
        );

        fs::write(&test_path, &test_content)
            .map_err(|e| format!("Kann {} nicht schreiben: {}", test_path.display(), e))?;

        println!("✅ Test-Datei erstellt: {}", test_path.display());
    }

    // Instruktionen ausgeben
    println!();
    println!("┌───────────────────────────────────────────────────────────┐");
    println!("│  Nächste Schritte (AUDIT_INTAKE_PROTOCOL.md):            │");
    println!("│                                                           │");
    println!("│  1. Öffne die Test-Datei(en) und schreibe den roten Test │");
    println!("│  2. Verifiziere: Test MUSS fehlschlagen                  │");
    for krate in &affected_crates {
        println!(
            "│     cargo test -p {} --test audit_fix_{}    │",
            krate,
            short_hash
        );
    }
    println!("│  3. Implementiere den Fix                                │");
    println!("│  4. Verifiziere: Test MUSS jetzt grün sein               │");
    println!("│  5. Führe `just check` aus                               │");
    println!("└───────────────────────────────────────────────────────────┘");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_hash_rejected() {
        let result = run_init_audit_fix("xyz");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Ungültiger Commit-Hash"));
    }

    #[test]
    fn test_short_hash_rejected() {
        let result = run_init_audit_fix("abc");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Ungültiger Commit-Hash"));
    }

    #[test]
    fn test_nonexistent_commit_rejected() {
        // Ein syntaktisch gültiger aber nicht existierender Hash
        let result = run_init_audit_fix("deadbeefdeadbeef");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("nicht gefunden"));
    }
}
