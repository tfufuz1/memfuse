//! Subcommand to verify that all xtask subcommands referenced in GitHub workflow files
//! actually exist as match arms in `xtask/src/main.rs`.

use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Extracts all valid xtask subcommand names defined in `xtask/src/main.rs`.
pub fn extract_valid_subcommands(main_rs_content: &str) -> HashSet<String> {
    let mut valid_commands = HashSet::new();
    let arm_regex = Regex::new(r#"^\s*"([a-z0-9_-]+)"\s*=>"#).expect("Valid regex");

    for line in main_rs_content.lines() {
        if let Some(captures) = arm_regex.captures(line) {
            if let Some(cmd) = captures.get(1) {
                valid_commands.insert(cmd.as_str().to_string());
            }
        }
    }

    valid_commands
}

/// Structure representing a subcommand invocation found in a workflow file.
#[derive(Debug, PartialEq, Eq)]
pub struct WorkflowCommandInvocation {
    pub file_path: String,
    pub line_number: usize,
    pub command: String,
}

/// Parses workflow YAML content to find `cargo run -p xtask -- <cmd>` and `cargo xtask <cmd>` invocations.
pub fn parse_workflow_invocations(
    file_path: &str,
    content: &str,
) -> Vec<WorkflowCommandInvocation> {
    let mut invocations = Vec::new();
    let cmd_regex =
        Regex::new(r"(?:cargo\s+run\s+-p\s+xtask\s+--\s+|cargo\s+xtask\s+)([a-z0-9_-]+)")
            .expect("Valid regex");

    for (idx, line) in content.lines().enumerate() {
        for captures in cmd_regex.captures_iter(line) {
            if let Some(cmd_match) = captures.get(1) {
                invocations.push(WorkflowCommandInvocation {
                    file_path: file_path.to_string(),
                    line_number: idx + 1,
                    command: cmd_match.as_str().to_string(),
                });
            }
        }
    }

    invocations
}

/// Runs the workflow command sync check against the repository root.
pub fn run_check_workflow_commands(root: &Path) -> bool {
    println!("=== Running Meta-Gate: check-workflow-commands ===");

    let main_rs_path = root.join("xtask/src/main.rs");
    let main_rs_content = match fs::read_to_string(&main_rs_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌ Failed to read {}: {}", main_rs_path.display(), e);
            return false;
        }
    };

    let valid_subcommands = extract_valid_subcommands(&main_rs_content);
    if valid_subcommands.is_empty() {
        eprintln!(
            "❌ No valid xtask subcommands extracted from {}",
            main_rs_path.display()
        );
        return false;
    }

    println!(
        "Found {} valid xtask subcommands in main.rs",
        valid_subcommands.len()
    );

    let workflows_dir = root.join(".github/workflows");
    if !workflows_dir.exists() {
        eprintln!(
            "⚠️ Workflows directory does not exist: {}",
            workflows_dir.display()
        );
        return true;
    }

    let mut all_invocations = Vec::new();

    for entry in WalkDir::new(&workflows_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "yml" || ext == "yaml" {
                    let relative_path = path
                        .strip_prefix(root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string();

                    match fs::read_to_string(path) {
                        Ok(content) => {
                            let invs = parse_workflow_invocations(&relative_path, &content);
                            all_invocations.extend(invs);
                        }
                        Err(e) => {
                            eprintln!("⚠️ Could not read workflow file {}: {}", relative_path, e);
                        }
                    }
                }
            }
        }
    }

    let mut missing_count = 0;

    for inv in &all_invocations {
        if !valid_subcommands.contains(&inv.command) {
            eprintln!(
                "❌ {}:{}: Unknown xtask subcommand '{}'",
                inv.file_path, inv.line_number, inv.command
            );
            missing_count += 1;
        }
    }

    if missing_count > 0 {
        eprintln!(
            "❌ [META-GATE FAILED]: {} unknown xtask subcommand invocation(s) found in workflows",
            missing_count
        );
        false
    } else {
        println!(
            "✅ [META-GATE PASSED]: Verified {} xtask subcommand invocation(s) across workflows",
            all_invocations.len()
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_valid_workflow() {
        let dir = tempdir().expect("tempdir creation");
        let root = dir.path();

        let xtask_src = root.join("xtask/src");
        fs::create_dir_all(&xtask_src).expect("create_dir_all");
        let main_rs_content = r#"
fn main() {
    match subcommand {
        "sync-docs" => {}
        "check-compile" => {}
        "check-dag" => {}
        _ => {}
    }
}
"#;
        fs::write(xtask_src.join("main.rs"), main_rs_content).expect("write main.rs");

        let workflows_dir = root.join(".github/workflows");
        fs::create_dir_all(&workflows_dir).expect("create workflows dir");
        let workflow_content = r#"
name: Test Workflow
jobs:
  test:
    steps:
      - name: Step 1
        run: cargo run -p xtask -- check-compile
      - name: Step 2
        run: cargo xtask check-dag
"#;
        fs::write(workflows_dir.join("test.yml"), workflow_content).expect("write workflow.yml");

        assert!(run_check_workflow_commands(root));
    }

    #[test]
    fn test_invalid_workflow() {
        let dir = tempdir().expect("tempdir creation");
        let root = dir.path();

        let xtask_src = root.join("xtask/src");
        fs::create_dir_all(&xtask_src).expect("create_dir_all");
        let main_rs_content = r#"
fn main() {
    match subcommand {
        "sync-docs" => {}
        "check-compile" => {}
        _ => {}
    }
}
"#;
        fs::write(xtask_src.join("main.rs"), main_rs_content).expect("write main.rs");

        let workflows_dir = root.join(".github/workflows");
        fs::create_dir_all(&workflows_dir).expect("create workflows dir");
        let workflow_content = r#"
name: Test Workflow
jobs:
  test:
    steps:
      - name: Step 1
        run: cargo run -p xtask -- check-compile
      - name: Step 2
        run: cargo run -p xtask -- non-existent-command
"#;
        fs::write(workflows_dir.join("test.yml"), workflow_content).expect("write workflow.yml");

        assert!(!run_check_workflow_commands(root));
    }
}
