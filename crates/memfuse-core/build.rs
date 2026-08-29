use std::path::Path;
use std::process::Command;

fn main() {
    let schema_path = "../../schemas/memfuse.fbs";
    let out_dir = "src/ipc";

    if Path::new(schema_path).exists() {
        println!("cargo:rerun-if-changed={}", schema_path);

        let output_file = Path::new(out_dir).join("memfuse_generated.rs");

        // Try to run flatc, but don't panic if it's missing but output already exists
        let flatc_exists = Command::new("flatc")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if flatc_exists {
            let status = Command::new("flatc")
                .args(["--rust", "-o", out_dir, schema_path])
                .status()
                .expect("Failed to execute flatc");

            if !status.success() {
                panic!("flatc failed to generate code");
            }
        } else if !output_file.exists() {
            panic!("flatc not found and generated code does not exist. Please install flatbuffers compiler.");
        } else {
            // AI-TAG[SPEC-DRIFT][MINOR] RESOLVED: flatc binary missing in environment, falling back to pre-generated ipc/memfuse_generated.rs (TS:2026-08-29T12:00:00Z) (SESSION: a3f29c1d)
            println!("cargo:warning=flatc not found, using existing generated code.");
        }
    }
}
