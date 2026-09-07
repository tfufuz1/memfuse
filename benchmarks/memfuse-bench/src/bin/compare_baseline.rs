// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Standalone CLI Tooling für den Vergleich aktueller Benchmark-Ergebnisse mit der Baseline
// INVARIANTEN: Exit-Code 1 bei Regression > Schwellwert, Exit-Code 0 bei Erfolg, klare Human-Readable Logs.

use memfuse_bench::compare::compare_metrics_files;
use std::env;
use std::path::PathBuf;
use std::process::exit;

fn print_usage() {
    println!("Usage: compare-baseline [OPTIONS]");
    println!("Options:");
    println!("  --current <PATH>    Path to current evaluation metrics JSON (default: benchmarks/results/current_metrics.json)");
    println!("  --baseline <PATH>   Path to baseline metrics JSON (default: benchmarks/memfuse-bench/baseline_metrics.json)");
    println!("  --threshold <FLOAT> Relative tolerance threshold (default: 0.05 for 5%)");
    println!("  --help              Display this help message");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut current_path = PathBuf::from("benchmarks/results/current_metrics.json");
    let mut baseline_path = PathBuf::from("benchmarks/memfuse-bench/baseline_metrics.json");
    let mut threshold = 0.05f64;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--current" => {
                if i + 1 < args.len() {
                    current_path = PathBuf::from(&args[i + 1]);
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
                    } else {
                        eprintln!("[ERROR] Invalid threshold parameter: {}", args[i + 1]);
                        exit(1);
                    }
                    i += 1;
                }
            }
            "--help" | "-h" => {
                print_usage();
                exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    println!("=== Retrieval Quality Regression Gate Comparison ===");
    println!("Current Metrics : {}", current_path.display());
    println!("Baseline Metrics: {}", baseline_path.display());
    println!("Tolerance Drop  : {:.2}%\n", threshold * 100.0);

    match compare_metrics_files(&current_path, &baseline_path, threshold) {
        Ok(res) => {
            for pass in &res.pass_messages {
                println!("{}", pass);
            }
            for hint in &res.info_hints {
                println!("{}", hint);
            }

            if res.has_regression {
                eprintln!("\n❌ REGRESSION GATE FAILED!");
                for err in &res.errors {
                    eprintln!("{}", err);
                }
                exit(1);
            } else {
                println!("\n✅ REGRESSION GATE PASSED! All metrics within acceptable threshold.");
                exit(0);
            }
        }
        Err(e) => {
            eprintln!("[ERROR] Comparison failed: {}", e);
            exit(1);
        }
    }
}
