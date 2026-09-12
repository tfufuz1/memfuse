//! Pre-Submit Gate Executor (P0-2)
//!
//! Führt alle Schritte der Phase 6 (Rebase-Check, Compile, Phantom-File-Check,
//! Claim-Release, Stale-Claim-Cleanup) fehlerrobust nacheinander aus und
//! signalisiert das Gesamtergebnis.

use std::process::Command;

pub fn run_jules_submit_gate(crate_name: Option<&str>) -> bool {
    println!("=== Phase 6: Jules Pre-Submit Gate ===");
    let mut all_passed = true;

    // ── 6.1 REBASE-CHECK ──────────────────────────────────────────────────
    println!("→ [6.1 Rebase-Check]: git fetch origin main...");
    let fetch_status = Command::new("git")
        .args(["fetch", "origin", "main"])
        .status();

    let rebase_ok = match fetch_status {
        Ok(st) if st.success() => {
            let ancestor_status = Command::new("git")
                .args(["merge-base", "--is-ancestor", "origin/main", "HEAD"])
                .status();
            match ancestor_status {
                Ok(ast) if ast.success() => true,
                _ => false,
            }
        }
        _ => false,
    };

    if rebase_ok {
        println!("✅ [6.1 Rebase-Check]: Branch ist aktuell gegenüber origin/main.");
    } else {
        eprintln!("❌ [6.1 Rebase-Check]: Branch ist nicht aktuell gegenüber origin/main.");
        eprintln!("   Bitte 'git rebase origin/main' ausführen und erneut versuchen.");
        all_passed = false;
    }

    // ── 6.2 COMPILE-VERIFIKATION ─────────────────────────────────────────
    println!("→ [6.2 Compile-Check]: cargo check --workspace --exclude memfuse-tauri --quiet...");
    let check_status = Command::new("cargo")
        .args([
            "check",
            "--workspace",
            "--exclude",
            "memfuse-tauri",
            "--quiet",
        ])
        .status();

    match check_status {
        Ok(st) if st.success() => {
            println!("✅ [6.2 Compile-Check]: Workspace kompiliert fehlerfrei.");
        }
        _ => {
            eprintln!("❌ [6.2 Compile-Check]: cargo check fehlgeschlagen.");
            all_passed = false;
        }
    }

    // ── 6.3 PHANTOM-FILE-CHECK ────────────────────────────────────────────
    println!("→ [6.3 Phantom-File-Check]: Prüfe behauptete Dateien im Diff...");
    if crate::check_phantom_files::run_check_phantom_files() {
        println!("✅ [6.3 Phantom-File-Check]: Keine Phantom-Dateien gefunden.");
    } else {
        eprintln!("❌ [6.3 Phantom-File-Check]: Phantom-Dateien erkannt.");
        all_passed = false;
    }

    // Abbrechen, falls ein Pflichtschritt (6.1–6.3) fehlgeschlagen ist
    if !all_passed {
        eprintln!("❌ SUBMIT GATE FEHLEGESCHLAGEN — Vor 6.4/6.5 abgebaut.");
        return false;
    }

    // ── 6.4 CLAIM-RELEASE ────────────────────────────────────────────────
    if let Some(c) = crate_name {
        println!("→ [6.4 Claim-Release]: Gebe Claim für Crate '{}' frei...", c);
        let release_ok = crate::claim::run_release_local(&["--crate".to_string(), c.to_string()]);
        if release_ok {
            println!("✅ [6.4 Claim-Release]: Claim für '{}' erfolgreich freigegeben.", c);
        } else {
            eprintln!("❌ [6.4 Claim-Release]: Freigabe für '{}' fehlgeschlagen.", c);
            all_passed = false;
        }
    } else {
        println!("ℹ️ [6.4 Claim-Release]: Übersprungen (kein --crate angegeben).");
    }

    // ── 6.5 STALE-CLAIM CLEANUP (P0-1) ────────────────────────────────────
    // TODO(P0-1): expire_stale_claims aufrufen sobald P0-1 gemerged ist
    println!("ℹ️ [6.5 Stale-Claim-Cleanup]: Übersprungen (Wartet auf P0-1 Merge).");

    if all_passed {
        println!("════════════════════════════════════════════════");
        println!("✅ SUBMIT GATE BESTANDEN — Submit erlaubt.");
        println!("════════════════════════════════════════════════");
        true
    } else {
        eprintln!("❌ SUBMIT GATE FEHLEGESCHLAGEN.");
        false
    }
}
