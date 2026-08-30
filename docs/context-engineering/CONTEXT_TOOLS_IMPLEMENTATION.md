# Context Tools — Implementation Specifications
**Rust-basierte CLI-Tools für Jules Kontext-Optimierung**

Version: 2.0 · Status: READY_FOR_DEV
Target: `xtask/src/` integration · Language: Rust (sync, no async)
Performance: O(n) file scan, < 5 sec per crate

---

## 1. ARCHITECTURE OVERVIEW

```
xtask/
├── src/
│   ├── main.rs                  # CLI router
│   ├── context/
│   │   ├── mod.rs               # Context trait + types
│   │   ├── digest.rs            # context-digest implementation
│   │   ├── tags.rs              # context-tags implementation
│   │   ├── file_context.rs      # context-file implementation
│   │   ├── crate_context.rs     # context-crate implementation
│   │   ├── parser.rs            # Tag parsing (delimiter-based)
│   │   └── output.rs            # JSON/NDJSON/text formatting
│   └── audit/
│       ├── mod.rs
│       ├── verify.rs            # audit-verify implementation
│       └── review.rs            # audit-review implementation
└── Cargo.toml
```

---

## 2. DATA STRUCTURES

### 2.1 Core Types

```rust
// xtask/src/context/mod.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// AI-TAG Status Enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TagStatus {
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "IN-PROGRESS")]
    InProgress,
    #[serde(rename = "DONE")]
    Done,
    #[serde(rename = "BLOCKED")]
    Blocked,
    #[serde(rename = "RESOLVED")]
    Resolved,
}

/// Severity Level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    #[serde(rename = "BLOCKER")]
    Blocker = 4,
    #[serde(rename = "CRITICAL")]
    Critical = 3,
    #[serde(rename = "MAJOR")]
    Major = 2,
    #[serde(rename = "MINOR")]
    Minor = 1,
}

/// AI-TAG Category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TagCategory {
    #[serde(rename = "CONCURRENCY")]
    Concurrency,
    #[serde(rename = "MEMORY-SAFETY")]
    MemorySafety,
    #[serde(rename = "ASYNC-IO")]
    AsyncIo,
    #[serde(rename = "CONVENTION-DRIFT")]
    ConventionDrift,
    #[serde(rename = "DOC-DRIFT")]
    DocDrift,
    #[serde(rename = "SECURITY")]
    Security,
    #[serde(rename = "PERFORMANCE")]
    Performance,
    #[serde(rename = "SMELL")]
    Smell,
    #[serde(rename = "ALG-FIX")]
    AlgFix,
    #[serde(rename = "DEBT")]
    Debt,
    #[serde(rename = "PANIC-SAFETY")]
    PanicSafety,
    #[serde(rename = "INTEGRATION")]
    Integration,
    #[serde(rename = "REFACTOR")]
    Refactor,
    #[serde(rename = "TEST")]
    Test,
    #[serde(rename = "PERF")]
    Perf,
}

/// Parsed AI-TAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTag {
    pub category: TagCategory,
    pub severity: Severity,
    pub short_description: String,
    pub id: String,                    // AGT-<CRATE>-<8hex>
    pub timestamp: String,             // ISO-8601-UTC
    pub session: String,               // 8-hex hash
    pub status: TagStatus,
    pub befund: Option<String>,
    pub risiko: Option<String>,
    pub empfehlung: Option<String>,
    pub audit_id: Option<String>,      // AUDIT-2026-09-001 if from audit
    pub file: PathBuf,
    pub line: usize,
}

/// Parsed ANCHOR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub anchor_type: String,           // INTEGRATION, DEBT, REFACTOR, etc.
    pub id: String,                    // Custom ID
    pub description: String,
    pub timestamp: String,
    pub session: String,
    pub status: TagStatus,
    pub gate: Option<String>,          // Cargo test gate
    pub depends_on: Vec<String>,       // comma-separated ANCHOR IDs
    pub agent: Option<String>,         // AGENT:N
    pub file: PathBuf,
    pub line: usize,
}

/// FILE-CONTEXT Header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContext {
    pub file: PathBuf,
    pub stand: String,                 // ISO-8601-UTC
    pub session: String,
    pub zweck: String,                 // Purpose
    pub scope: String,                 // Crate:X | Layer:Y | Role:Z
    pub invarianten: Vec<String>,
    pub nicht_offensichtlich: Vec<String>,
    pub siehe_auch: Vec<PathBuf>,
    pub agent_notiz: Option<String>,
}

/// REVIEW-PASS Entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPass {
    pub pass_number: usize,            // N from N/M
    pub total_passes: usize,           // M from N/M
    pub id: String,                    // AGT-<CRATE>-<8hex>
    pub timestamp: String,
    pub session: String,
    pub status: ReviewPassStatus,      // PASS | FAIL | CONDITIONAL
    pub kontext: ReviewContext,        // FRESH | CARRIED_FORWARD
    pub befund: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewPassStatus {
    #[serde(rename = "PASS")]
    Pass,
    #[serde(rename = "FAIL")]
    Fail,
    #[serde(rename = "CONDITIONAL")]
    Conditional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewContext {
    #[serde(rename = "FRESH")]
    Fresh,
    #[serde(rename = "CARRIED_FORWARD")]
    CarriedForward,
}

/// Context Digest (für context-digest Befehl)
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextDigest {
    pub timestamp: String,
    pub session: String,
    pub blockers: Vec<AiTag>,
    pub open_anchors: Vec<Anchor>,
    pub crate_stats: std::collections::BTreeMap<String, CrateStats>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrateStats {
    pub blockers: usize,
    pub criticals: usize,
    pub anchors: usize,
}
```

### 2.2 Parser Types

```rust
// xtask/src/context/parser.rs

use regex::Regex;
use crate::context::{AiTag, Anchor, FileContext, TagStatus, Severity, TagCategory};

/// Parser für delimiter-basierte Tag-Grammatik (KEIN REGEX für Hauptparsing)
pub struct TagParser {
    // Pre-compiled regex für ID-Extraktion nur (minimales overhead)
    id_regex: Regex,
    ts_regex: Regex,
}

impl TagParser {
    pub fn new() -> Self {
        TagParser {
            id_regex: Regex::new(r"ID:\s*(AGT-[A-Z]+-[0-9a-f]{8})").unwrap(),
            ts_regex: Regex::new(r"TS:\s*(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z)").unwrap(),
        }
    }

    /// Parse AI-TAG from comment block (delimiter-based, fast)
    pub fn parse_ai_tag(
        &self,
        lines: &[&str],
        file: &Path,
        line_no: usize,
    ) -> Result<AiTag, String> {
        // Struktur:
        // // AI-TAG[KATEGORIE][SEVERITY] Kurzbeschreibung
        // // ID:       AGT-<CRATE>-<8hex>
        // // TS:       2026-08-29T09:14:07Z
        // // SESSION:  a3f29c1d
        // // STATUS:   OPEN
        // // BEFUND:   ...
        // // RISIKO:   ...
        // // EMPFEHLUNG: ...

        if lines.is_empty() {
            return Err("Empty AI-TAG".to_string());
        }

        let first_line = lines[0].trim_start_matches("//").trim();

        // Parse first line: AI-TAG[KATEGORIE][SEVERITY] Description
        let parts: Vec<&str> = first_line.split(']').collect();
        if parts.len() < 3 {
            return Err("Invalid AI-TAG format".to_string());
        }

        let category_str = parts[0].trim_start_matches("AI-TAG[").trim();
        let severity_str = parts[1].trim_start_matches('[').trim();
        let description = parts[2..].join("]").trim().to_string();

        let category = Self::parse_category(category_str)?;
        let severity = Self::parse_severity(severity_str)?;

        // Parse key-value fields
        let mut tag_data = std::collections::BTreeMap::new();
        for line in &lines[1..] {
            let trimmed = line.trim_start_matches("//").trim();
            if let Some((key, value)) = trimmed.split_once(':') {
                tag_data.insert(key.trim(), value.trim());
            }
        }

        Ok(AiTag {
            category,
            severity,
            short_description: description,
            id: tag_data.get("ID").ok_or("Missing ID field")?.to_string(),
            timestamp: tag_data.get("TS").ok_or("Missing TS field")?.to_string(),
            session: tag_data.get("SESSION").ok_or("Missing SESSION field")?.to_string(),
            status: Self::parse_status(tag_data.get("STATUS").map(|s| *s))?,
            befund: tag_data.get("BEFUND").map(|s| s.to_string()),
            risiko: tag_data.get("RISIKO").map(|s| s.to_string()),
            empfehlung: tag_data.get("EMPFEHLUNG").map(|s| s.to_string()),
            audit_id: tag_data.get("AUDIT_ID").map(|s| s.to_string()),
            file: file.to_path_buf(),
            line: line_no,
        })
    }

    // Similar for parse_anchor, parse_file_context, etc.
    // Alle delimiter-based (fast)
}
```

---

## 3. COMMAND IMPLEMENTATIONS

### 3.1 `cargo xtask context-digest`

```rust
// xtask/src/context/digest.rs

use crate::context::{ContextDigest, CrateStats, AiTag, Anchor, TagStatus, Severity};
use std::path::Path;
use std::collections::BTreeMap;

pub fn context_digest(
    crate_filter: Option<&str>,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Scan crates/ für alle AI-TAGs und ANCHORs
    let mut all_tags = Vec::new();
    let mut all_anchors = Vec::new();
    let mut crate_stats = BTreeMap::new();

    for entry in walkdir::WalkDir::new("crates")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        if let Some(cf) = crate_filter {
            if !entry.path().to_string_lossy().contains(cf) {
                continue;
            }
        }

        // Parse file for tags
        let content = std::fs::read_to_string(entry.path())?;
        let lines: Vec<&str> = content.lines().collect();

        // Scan line-by-line untuk AI-TAG lines
        for (line_no, line) in lines.iter().enumerate() {
            if line.contains("// AI-TAG[") {
                // Collect tag + following lines until next AI-TAG/ANCHOR/comment block
                let tag_lines = collect_tag_block(&lines, line_no);
                match TagParser::new().parse_ai_tag(&tag_lines, entry.path(), line_no + 1) {
                    Ok(tag) => {
                        extract_crate_name(entry.path())
                            .and_then(|crate_name| {
                                let stats = crate_stats.entry(crate_name).or_insert_with(|| CrateStats {
                                    blockers: 0,
                                    criticals: 0,
                                    anchors: 0,
                                });

                                match tag.severity {
                                    Severity::Blocker => stats.blockers += 1,
                                    Severity::Critical => stats.criticals += 1,
                                    _ => {}
                                }
                                Some(())
                            });
                        all_tags.push(tag);
                    }
                    Err(e) => eprintln!("Failed to parse tag at {}: {}", entry.path().display(), e),
                }
            }

            if line.contains("// ANCHOR[") {
                let anchor_lines = collect_tag_block(&lines, line_no);
                match TagParser::new().parse_anchor(&anchor_lines, entry.path(), line_no + 1) {
                    Ok(anchor) => {
                        if let Some(crate_name) = extract_crate_name(entry.path()) {
                            let stats = crate_stats.entry(crate_name).or_insert_with(|| CrateStats {
                                blockers: 0,
                                criticals: 0,
                                anchors: 0,
                            });
                            stats.anchors += 1;
                        }
                        all_anchors.push(anchor);
                    }
                    Err(e) => eprintln!("Failed to parse anchor at {}: {}", entry.path().display(), e),
                }
            }
        }
    }

    // 2. Filter für Blockers + Criticals
    let blockers_and_crits: Vec<_> = all_tags
        .iter()
        .filter(|tag| {
            tag.severity >= Severity::Critical && tag.status != TagStatus::Resolved
        })
        .cloned()
        .collect();

    // 3. Filter open ANCHORs
    let open_anchors: Vec<_> = all_anchors
        .iter()
        .filter(|a| matches!(a.status, TagStatus::Open | TagStatus::InProgress))
        .cloned()
        .collect();

    // 4. Build digest
    let digest = ContextDigest {
        timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        session: std::env::var("JULIUS_SESSION_ID").unwrap_or_else(|_| "unknown".to_string()),
        blockers: blockers_and_crits,
        open_anchors,
        crate_stats,
    };

    // 5. Output based on format
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&digest)?);
        }
        "text" => {
            println!("=== CRITICAL BLOCKERS ===");
            for tag in &digest.blockers {
                println!("  {} {} ({})", tag.id, tag.category, tag.severity);
                println!("    Location: {}:{}", tag.file.display(), tag.line);
                if let Some(befund) = &tag.befund {
                    println!("    Befund: {}", befund);
                }
            }
            println!("\n=== OPEN ANCHORS ===");
            for anchor in &digest.open_anchors {
                println!("  {} {}", anchor.id, anchor.description);
            }
        }
        _ => return Err(format!("Unknown format: {}", format).into()),
    }

    Ok(())
}

fn collect_tag_block(lines: &[&str], start: usize) -> Vec<&str> {
    // Collect current line + following lines that start with // and are part of the block
    let mut block = vec![lines[start]];
    for line in &lines[start + 1..] {
        let trimmed = line.trim();
        if trimmed.starts_with("//") && !trimmed.starts_with("// AI-TAG") && !trimmed.starts_with("// ANCHOR") {
            block.push(line);
        } else if trimmed.is_empty() {
            break;
        } else {
            break;
        }
    }
    block
}

fn extract_crate_name(path: &Path) -> Option<String> {
    // Extrahiere "memfuse-db" aus "crates/memfuse-db/src/..."
    path.components()
        .find_map(|c| {
            if let std::path::Component::Normal(name) = c {
                let s = name.to_string_lossy();
                if s.starts_with("memfuse-") {
                    return Some(s.to_string());
                }
            }
            None
        })
}
```

### 3.2 `cargo xtask context-tags --filter`

```rust
// xtask/src/context/tags.rs

use crate::context::{AiTag, Severity, TagStatus};

pub struct TagFilter {
    pub crate_name: Option<String>,
    pub severity: Option<Severity>,
    pub status: Option<TagStatus>,
    pub tag_type: Option<String>, // "AI-TAG" or "ANCHOR"
}

pub fn context_tags(filter: TagFilter, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut results = Vec::new();

    // Scan crates/ (same as digest.rs)
    for entry in walkdir::WalkDir::new("crates")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        // Apply crate filter
        if let Some(ref cf) = filter.crate_name {
            if !entry.path().to_string_lossy().contains(cf) {
                continue;
            }
        }

        let content = std::fs::read_to_string(entry.path())?;
        let lines: Vec<&str> = content.lines().collect();

        for (line_no, line) in lines.iter().enumerate() {
            if let Some(ref tag_type) = filter.tag_type {
                if tag_type == "AI-TAG" && !line.contains("// AI-TAG[") {
                    continue;
                }
                if tag_type == "ANCHOR" && !line.contains("// ANCHOR[") {
                    continue;
                }
            }

            if line.contains("// AI-TAG[") {
                let tag_lines = collect_tag_block(&lines, line_no);
                match TagParser::new().parse_ai_tag(&tag_lines, entry.path(), line_no + 1) {
                    Ok(tag) => {
                        // Apply filters
                        if let Some(ref sev) = filter.severity {
                            if tag.severity != *sev {
                                continue;
                            }
                        }
                        if let Some(ref st) = filter.status {
                            if tag.status != *st {
                                continue;
                            }
                        }
                        results.push(serde_json::to_value(&tag)?);
                    }
                    Err(_) => {} // Skip parse errors
                }
            }
        }
    }

    // Output
    match format {
        "json" => {
            let arr = serde_json::Value::Array(results);
            println!("{}", serde_json::to_string_pretty(&arr)?);
        }
        "ndjson" => {
            for result in results {
                println!("{}", result.to_string());
            }
        }
        "text" => {
            for result in results {
                if let Some(obj) = result.as_object() {
                    println!(
                        "{} {} {} ({}:{})",
                        obj.get("id").and_then(|v| v.as_str()).unwrap_or("?"),
                        obj.get("severity").and_then(|v| v.as_str()).unwrap_or("?"),
                        obj.get("status").and_then(|v| v.as_str()).unwrap_or("?"),
                        obj.get("file").and_then(|v| v.as_str()).unwrap_or("?"),
                        obj.get("line").and_then(|v| v.as_u64()).unwrap_or(0),
                    );
                }
            }
        }
        _ => return Err(format!("Unknown format: {}", format).into()),
    }

    Ok(())
}
```

### 3.3 `cargo xtask context-file`

```rust
// xtask/src/context/file_context.rs

pub fn context_file(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path).into());
    }

    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();

    println!("=== FILE CONTEXT HEADER ===");
    // Find FILE-CONTEXT block
    for (line_no, line) in lines.iter().enumerate() {
        if line.contains("// FILE-CONTEXT") {
            let fc_lines = collect_tag_block(&lines, line_no);
            match TagParser::new().parse_file_context(&fc_lines, path, line_no + 1) {
                Ok(fc) => {
                    println!("STAND:    {}", fc.stand);
                    println!("ZWECK:    {}", fc.zweck);
                    println!("SCOPE:    {}", fc.scope);
                    println!("INVARIANTEN:");
                    for inv in &fc.invarianten {
                        println!("  - {}", inv);
                    }
                    println!("NICHT-OFFENSICHTLICH:");
                    for no in &fc.nicht_offensichtlich {
                        println!("  - {}", no);
                    }
                }
                Err(e) => println!("Failed to parse FILE-CONTEXT: {}", e),
            }
            break;
        }
    }

    println!("\n=== OPEN ISSUES (THIS FILE) ===");
    // Extract all AI-TAGs and ANCHORs from this file
    for (line_no, line) in lines.iter().enumerate() {
        if line.contains("// AI-TAG[") {
            let tag_lines = collect_tag_block(&lines, line_no);
            match TagParser::new().parse_ai_tag(&tag_lines, path, line_no + 1) {
                Ok(tag) => {
                    println!("AI-TAG[{}][{}] {} (ID: {})", tag.category, tag.severity, tag.short_description, tag.id);
                    if !matches!(tag.status, TagStatus::Resolved) {
                        println!("  STATUS: {:?}", tag.status);
                        if let Some(befund) = &tag.befund {
                            println!("  BEFUND: {}", befund);
                        }
                    }
                }
                Err(_) => {}
            }
        }
        if line.contains("// ANCHOR[") {
            let anchor_lines = collect_tag_block(&lines, line_no);
            match TagParser::new().parse_anchor(&anchor_lines, path, line_no + 1) {
                Ok(anchor) => {
                    println!("ANCHOR[{}] {} (STATUS: {})", anchor.anchor_type, anchor.id, anchor.status);
                }
                Err(_) => {}
            }
        }
    }

    println!("\n=== RUSTDOC EXCERPT ===");
    // Extract rustdoc comments from start of file
    for line in lines.iter().take(50) {
        if line.trim().starts_with("///") {
            println!("{}", line.trim_start_matches("///").trim());
        }
    }

    Ok(())
}
```

---

## 4. CLI ROUTER

```rust
// xtask/src/main.rs

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "MemFuse xtask")]
#[command(about = "Build automation and context tools for Jules")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract and display context digest
    ContextDigest {
        #[arg(long, short)]
        crate_name: Option<String>,

        #[arg(long, short, default_value = "json")]
        format: String,
    },

    /// Filter and extract tags
    ContextTags {
        #[arg(long, short)]
        crate_name: Option<String>,

        #[arg(long, short)]
        severity: Option<String>,

        #[arg(long, short)]
        status: Option<String>,

        #[arg(long, short)]
        tag_type: Option<String>,

        #[arg(long, short, default_value = "json")]
        format: String,
    },

    /// Show context for a specific file
    ContextFile {
        #[arg(value_name = "PATH")]
        file: String,
    },

    /// Show context for a specific crate
    ContextCrate {
        #[arg(value_name = "CRATE")]
        crate_name: String,

        #[arg(long, short, default_value = "json")]
        format: String,
    },

    /// Verify audit finding validity
    AuditVerify {
        #[arg(value_name = "FINDING_ID")]
        finding_id: String,

        #[arg(long)]
        file: Option<String>,

        #[arg(long)]
        line: Option<usize>,
    },

    /// Log audit fix completion
    AuditReview {
        #[arg(value_name = "FINDING_ID")]
        finding_id: String,

        #[arg(long, short)]
        status: String,

        #[arg(long)]
        note: Option<String>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::ContextDigest { crate_name, format } => {
            crate::context::digest::context_digest(crate_name.as_deref(), &format)?;
        }
        Commands::ContextTags {
            crate_name,
            severity,
            status,
            tag_type,
            format,
        } => {
            let filter = crate::context::tags::TagFilter {
                crate_name,
                severity: severity.and_then(|s| parse_severity(&s)),
                status: status.and_then(|s| parse_status(&s)),
                tag_type,
            };
            crate::context::tags::context_tags(filter, &format)?;
        }
        Commands::ContextFile { file } => {
            crate::context::file_context::context_file(&file)?;
        }
        Commands::ContextCrate { crate_name, format } => {
            crate::context::crate_context::context_crate(&crate_name, &format)?;
        }
        Commands::AuditVerify {
            finding_id,
            file,
            line,
        } => {
            crate::audit::verify::audit_verify(&finding_id, file.as_deref(), line)?;
        }
        Commands::AuditReview {
            finding_id,
            status,
            note,
        } => {
            crate::audit::review::audit_review(&finding_id, &status, note.as_deref())?;
        }
    }

    Ok(())
}
```

---

## 5. TESTING STRATEGY

```rust
// xtask/src/context/parser.rs (test module)

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_ai_tag_valid() {
        let lines = vec![
            "// AI-TAG[CONCURRENCY][CRITICAL] Race condition in relate()",
            "// ID:      AGT-DB-a3f29c1d",
            "// TS:      2026-08-29T09:14:07Z",
            "// SESSION: a3f29c1d",
            "// STATUS:  OPEN",
            "// BEFUND:  Concurrent access without lock",
        ];

        let parser = TagParser::new();
        let tag = parser.parse_ai_tag(&lines, &PathBuf::from("test.rs"), 1).unwrap();

        assert_eq!(tag.id, "AGT-DB-a3f29c1d");
        assert_eq!(tag.session, "a3f29c1d");
        assert_eq!(tag.timestamp, "2026-08-29T09:14:07Z");
        assert_eq!(tag.status, TagStatus::Open);
    }

    #[test]
    fn test_parse_ai_tag_missing_fields() {
        let lines = vec![
            "// AI-TAG[CONCURRENCY][CRITICAL] Race condition",
            "// ID: AGT-DB-a3f29c1d",
            // Missing TS, SESSION, STATUS
        ];

        let parser = TagParser::new();
        let result = parser.parse_ai_tag(&lines, &PathBuf::from("test.rs"), 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_anchor() {
        let lines = vec![
            "// ANCHOR[INTEGRATION:WP-7.1] Wire MarkdownChunker",
            "// TS:      2026-08-29T09:14:07Z",
            "// SESSION: a3f29c1d",
            "// STATUS:  IN-PROGRESS",
            "// GATE:    cargo test -p memfuse-db",
        ];

        let parser = TagParser::new();
        let anchor = parser.parse_anchor(&lines, &PathBuf::from("test.rs"), 1).unwrap();

        assert_eq!(anchor.anchor_type, "INTEGRATION");
        assert_eq!(anchor.id, "WP-7.1");
        assert_eq!(anchor.status, TagStatus::InProgress);
    }
}
```

---

## 6. INTEGRATION WITH EXISTING CODEBASE

### 6.1 Cargo.toml Dependencies

```toml
[dependencies]
clap = { version = "4.4", features = ["derive"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
walkdir = "2"
regex = "1"
chrono = "0.4"
```

### 6.2 Module Structure in xtask

```rust
// xtask/src/lib.rs

pub mod context;
pub mod audit;

// Re-export
pub use context::{AiTag, Anchor, FileContext, ContextDigest};
pub use audit::{AuditFinding, AuditResult};
```

---

## 7. PERFORMANCE TARGETS & BENCHMARKS

```rust
// benches/context_performance.rs

#[bench]
fn bench_parse_tags(b: &mut Bencher) {
    let code = include_str!("../crates/memfuse-db/src/collection/relate.rs");
    b.iter(|| {
        let parser = TagParser::new();
        let lines: Vec<&str> = code.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("// AI-TAG[") {
                let block = collect_tag_block(&lines, i);
                let _ = parser.parse_ai_tag(&block, &PathBuf::from("test.rs"), i + 1);
            }
        }
    });
}

// Expected: O(n) where n = lines of code
// Target: < 100 ns per line, < 5 sec per crate
```

---

## 8. DEPLOYMENT CHECKLIST

- [ ] All tests pass: `cargo test --package xtask`
- [ ] `cargo xtask context-digest` works on full workspace
- [ ] Output format validation (JSON schema)
- [ ] Integration with GitHub Actions CI
- [ ] Documentation in justfile + AGENTS.md
- [ ] Example outputs in README
- [ ] Performance profiling on memfuse-full codebase

---

**END OF IMPLEMENTATION SPEC**

---

Autor: Context Engineering Team
Zielgruppe: Rust-Entwickler für xtask
Status: READY_FOR_IMPLEMENTATION
