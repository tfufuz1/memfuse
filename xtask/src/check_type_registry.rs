use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::Path;

use crate::{compute_crate_layers, find_root_dir, get_workspace_crates};

/// Ein Treffer im Code oder in der TYPE_REGISTRY.md.
#[derive(Debug, Serialize)]
pub struct TypeHit {
    #[serde(rename = "crate")]
    pub krate: String,
    pub file: String,
    pub line: usize,
    pub layer: u8,
    pub kind: String, // "struct", "enum", "trait", "type"
}

/// Gesamtergebnis der Typ-Prüfung.
#[derive(Debug, Serialize)]
pub struct TypeCheckResult {
    pub query: String,
    pub registry_hit: bool,
    pub registry_entries: Vec<String>,
    pub code_hits: Vec<TypeHit>,
    pub recommendation: String,
}

/// Prüft ob ein Typ bereits in TYPE_REGISTRY.md oder im Code existiert.
///
/// Gibt ein maschinenlesbares JSON-Ergebnis auf stdout aus.
/// Exit-Semantik: `true` = keine Kollision, `false` = potenzielle Kollision.
pub fn run_check_type_registry(type_name: &str) -> bool {
    let root = find_root_dir();
    let result = check_type_registry(&root, type_name);

    // JSON-Ausgabe
    if let Ok(json) = serde_json::to_string_pretty(&result) {
        println!("{}", json);
    }

    let has_hits = result.registry_hit || !result.code_hits.is_empty();

    if has_hits {
        eprintln!(
            "⚠️  Typ '{}' existiert bereits — bestehenden Typ erweitern statt Duplikat anlegen!",
            type_name
        );
    } else {
        println!("✅ Typ '{}' existiert noch nicht — Anlage möglich.", type_name);
    }

    !has_hits
}

/// Kernlogik: Durchsucht TYPE_REGISTRY.md und Code nach dem Typnamen.
pub fn check_type_registry(root: &Path, type_name: &str) -> TypeCheckResult {
    let mut result = TypeCheckResult {
        query: type_name.to_string(),
        registry_hit: false,
        registry_entries: Vec::new(),
        code_hits: Vec::new(),
        recommendation: String::new(),
    };

    // 1. TYPE_REGISTRY.md durchsuchen
    let registry_path = root.join("docs").join("TYPE_REGISTRY.md");
    if let Ok(content) = fs::read_to_string(&registry_path) {
        for line in content.lines() {
            if line.contains(type_name) {
                result.registry_hit = true;
                result.registry_entries.push(line.trim().to_string());
            }
        }
    }

    // 2. Code durchsuchen
    // Regex: struct/enum/type/trait Name, mit optionalem pub/pub(crate) Prefix
    let pattern = format!(
        r"(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|type)\s+{}\b",
        regex::escape(type_name)
    );
    let code_re = Regex::new(&pattern).unwrap();

    // Layer-Map aufbauen
    let mut crates = get_workspace_crates();
    let _ = compute_crate_layers(&mut crates);
    let layer_map: std::collections::HashMap<String, u8> =
        crates.iter().map(|c| (c.name.clone(), c.layer)).collect();

    for entry in walkdir::WalkDir::new(root.join("crates"))
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }

        if let Ok(content) = fs::read_to_string(path) {
            for (idx, line) in content.lines().enumerate() {
                if code_re.is_match(line) {
                    let rel_path = path
                        .strip_prefix(root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string();

                    // Crate-Name extrahieren
                    let crate_name = extract_crate_from_path(&rel_path);
                    let layer = layer_map.get(&crate_name).copied().unwrap_or(0);

                    // Kind extrahieren
                    let kind = if line.contains("struct ") {
                        "struct"
                    } else if line.contains("enum ") {
                        "enum"
                    } else if line.contains("trait ") {
                        "trait"
                    } else {
                        "type"
                    };

                    result.code_hits.push(TypeHit {
                        krate: crate_name,
                        file: rel_path,
                        line: idx + 1,
                        layer,
                        kind: kind.to_string(),
                    });
                }
            }
        }
    }

    // Empfehlung generieren
    result.recommendation = if result.registry_hit && !result.code_hits.is_empty() {
        format!(
            "Typ '{}' ist sowohl in TYPE_REGISTRY.md als auch im Code definiert. Bestehenden Typ erweitern oder Kollision per ADR begründen.",
            type_name
        )
    } else if result.registry_hit {
        format!(
            "Typ '{}' ist in TYPE_REGISTRY.md registriert. Prüfe, ob der bestehende Typ erweitert werden kann.",
            type_name
        )
    } else if !result.code_hits.is_empty() {
        let first = &result.code_hits[0];
        format!(
            "Typ '{}' existiert in {} (Layer {}). Bestehenden Typ erweitern statt Duplikat anlegen.",
            type_name, first.krate, first.layer
        )
    } else {
        format!(
            "Typ '{}' existiert weder in TYPE_REGISTRY.md noch im Code. Anlage möglich — nach Implementierung in TYPE_REGISTRY.md eintragen.",
            type_name
        )
    };

    result
}

/// Extrahiert den Crate-Namen aus einem relativen Pfad wie `crates/memfuse-core/src/types.rs`.
fn extract_crate_from_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 && parts[0] == "crates" {
        parts[1].to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_crate_from_path() {
        assert_eq!(
            extract_crate_from_path("crates/memfuse-core/src/types.rs"),
            "memfuse-core"
        );
        assert_eq!(
            extract_crate_from_path("crates/memfuse-db/src/collection/search.rs"),
            "memfuse-db"
        );
        assert_eq!(extract_crate_from_path("src/main.rs"), "");
    }

    #[test]
    fn test_check_type_registry_nonexistent() {
        let root = crate::find_root_dir();
        let result = check_type_registry(&root, "ZzNonExistentTypeXx42");
        assert!(!result.registry_hit);
        assert!(result.code_hits.is_empty());
        assert!(result.recommendation.contains("existiert weder"));
    }

    #[test]
    fn test_check_type_registry_known_type() {
        let root = crate::find_root_dir();
        // TenantId ist bekanntermaßen in memfuse-core definiert
        let result = check_type_registry(&root, "TenantId");
        // Sollte mindestens im Code gefunden werden
        assert!(
            result.registry_hit || !result.code_hits.is_empty(),
            "TenantId sollte in TYPE_REGISTRY.md oder im Code existieren"
        );
    }
}
