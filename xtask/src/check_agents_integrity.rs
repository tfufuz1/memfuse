use crate::{find_root_dir, get_workspace_crates};
use std::fs;
use std::path::Path;

/// Verifies factual integrity between `AGENTS.md` and the actual codebase state.
pub fn run_check_agents_integrity() -> bool {
    println!("=== xtask check-agents-integrity ===");
    let root = find_root_dir();
    let mut success = true;

    // 1. Check that every workspace crate has a local AGENTS.md
    let crates = get_workspace_crates();
    for c in &crates {
        let crate_agents_path = root.join(&c.path).join("AGENTS.md");
        if !crate_agents_path.exists() {
            eprintln!(
                "❌ [AGENTS-INTEGRITY]: Crate '{}' is missing its local AGENTS.md file at {}",
                c.name,
                crate_agents_path.display()
            );
            success = false;
        }
    }

    // 2. Read root AGENTS.md
    let root_agents_path = root.join("AGENTS.md");
    if !root_agents_path.exists() {
        eprintln!(
            "❌ [AGENTS-INTEGRITY]: Root AGENTS.md missing at {}",
            root_agents_path.display()
        );
        return false;
    }

    let agents_content = match fs::read_to_string(&root_agents_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "❌ [AGENTS-INTEGRITY]: Failed to read root AGENTS.md: {}",
                e
            );
            return false;
        }
    };

    // 3. Check for types/crates erroneously listed as missing ("FEHLT") when they are implemented
    let header_marker = "### Fehlt / Nicht integriert";
    if let Some(fehlt_start) = agents_content.find(header_marker) {
        let fehlt_section = &agents_content[fehlt_start..];
        let after_header = &fehlt_section[header_marker.len()..];
        let section_end = header_marker.len()
            + after_header
                .find("###")
                .unwrap_or_else(|| after_header.len());
        let fehlt_table = &fehlt_section[..section_end];

        for line in fehlt_table.lines() {
            if !line.trim().starts_with('|')
                || line.contains("Komponente / Feature")
                || line.contains("---|")
            {
                continue;
            }

            let cols: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
            if cols.len() >= 3 {
                let item_raw = cols[1];
                let item_clean = item_raw.trim_matches('`');

                // Check if item exists in crates/
                if is_type_or_crate_implemented(item_clean, &root) {
                    eprintln!(
                        "❌ [AGENTS-INTEGRITY]: AGENTS.md lists '{}' as missing (FEHLT), but it is implemented in the codebase!",
                        item_clean
                    );
                    success = false;
                }
            }
        }
    }

    if success {
        println!("=== xtask check-agents-integrity PASSED ===");
    } else {
        eprintln!("=== xtask check-agents-integrity FAILED ===");
    }

    success
}

fn is_type_or_crate_implemented(item: &str, root: &Path) -> bool {
    let crates_dir = root.join("crates");
    if !crates_dir.exists() {
        return false;
    }

    // If item is a crate name (e.g., memfuse-kv-bridge)
    if item.starts_with("memfuse-") {
        if root.join("crates").join(item).exists() {
            return true;
        }
    }

    // Search for Rust symbol definitions: pub struct <item>, pub enum <item>, pub trait <item>, etc.
    let struct_pat = format!("pub struct {}", item);
    let enum_pat = format!("pub enum {}", item);
    let trait_pat = format!("pub trait {}", item);
    let type_pat = format!("pub type {}", item);

    for entry in walkdir::WalkDir::new(&crates_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            if let Ok(content) = fs::read_to_string(path) {
                if content.contains(&struct_pat)
                    || content.contains(&enum_pat)
                    || content.contains(&trait_pat)
                    || content.contains(&type_pat)
                {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_type_or_crate_implemented_finds_edge_provenance() {
        let root = find_root_dir();
        assert!(is_type_or_crate_implemented("EdgeProvenance", &root));
    }

    #[test]
    fn test_is_type_or_crate_implemented_finds_kv_bridge() {
        let root = find_root_dir();
        assert!(is_type_or_crate_implemented("memfuse-kv-bridge", &root));
    }

    #[test]
    fn test_is_type_or_crate_implemented_nonexistent() {
        let root = find_root_dir();
        assert!(!is_type_or_crate_implemented("NonExistentType12345", &root));
    }
}
