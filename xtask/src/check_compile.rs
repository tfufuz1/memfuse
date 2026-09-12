//! Subkommando zur verifizierten Kompilierbarkeitsprüfung des Workspaces.
//!
//! Führt `cargo check --locked --workspace --all-targets`
//! aus, um festzustellen, ob alle Workspace-Crates und deren Targets (Tests, Benches,
//! Examples) fehlerfrei kompilieren.

use std::process::Command;
use std::time::Instant;

pub fn run_check_compile() -> bool {
    let start = Instant::now();
    println!("=== Running xtask check-compile ===");
    println!("Executing: cargo check --locked --workspace --all-targets");

    let status = Command::new("cargo")
        .args(["check", "--locked", "--workspace", "--all-targets"])
        .status();

    match status {
        Ok(st) if st.success() => {
            println!(
                "✅ [GATE-COMPILE]: Workspace kompiliert fehlerfrei ({:.2}s)",
                start.elapsed().as_secs_f64()
            );
            true
        }
        Ok(st) => {
            eprintln!(
                "❌ [GATE-COMPILE]: cargo check fehlgeschlagen mit Exit-Code: {:?}",
                st.code()
            );
            false
        }
        Err(e) => {
            eprintln!(
                "❌ [GATE-COMPILE]: Ausführung von cargo check fehlgeschlagen: {}",
                e
            );
            false
        }
    }
}
