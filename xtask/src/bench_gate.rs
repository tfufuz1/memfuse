// FILE-CONTEXT
// STAND: 2026-09-08 (SESSION: jules)
// ZWECK: Subcommand cargo xtask bench-gate for benchmark regression evaluation
// INVARIANTEN: Zero Panic, clear exit codes for CI governance.

use memfuse_bench::regression_gate::{run_regression_gate, DEFAULT_TOLERANCE_THRESHOLD};
use std::path::PathBuf;

pub fn run_bench_gate(args: &[String]) -> bool {
    println!("=== Running xtask bench-gate ===");

    let mut results_path = PathBuf::from("benchmarks/results/current_metrics.json");
    let mut baseline_path = PathBuf::from("benchmarks/memfuse-bench/baseline_metrics.json");
    let mut threshold = DEFAULT_TOLERANCE_THRESHOLD;
    let mut update_baseline = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--results" => {
                if i + 1 < args.len() {
                    results_path = PathBuf::from(&args[i + 1]);
                    i += 1;
                }
            }
            "--baseline" => {
                if i + 1 < args.len() {
                    baseline_path = PathBuf::from(&args[i + 1]);
                    i += 1;
                }
            }
            "--threshold" => {
                if i + 1 < args.len() {
                    if let Ok(t) = args[i + 1].parse::<f64>() {
                        threshold = t;
                    }
                    i += 1;
                }
            }
            "--update" | "--update-baseline" => {
                update_baseline = true;
            }
            _ => {}
        }
        i += 1;
    }

    println!("  Results path : {}", results_path.display());
    println!("  Baseline path: {}", baseline_path.display());
    println!("  Threshold    : {:.1}%", threshold * 100.0);
    println!("  Update mode  : {}", update_baseline);

    match run_regression_gate(&results_path, &baseline_path, threshold, update_baseline) {
        Ok(res) => res.passed,
        Err(e) => {
            eprintln!("❌ [BENCH-GATE ERROR]: {}", e);
            false
        }
    }
}
