import os

def fix_file(path, search, replace):
    if not os.path.exists(path):
        print(f"File not found: {path}")
        return
    with open(path, 'r') as f:
        content = f.read()
    if search in content:
        new_content = content.replace(search, replace)
        with open(path, 'w') as f:
            f.write(new_content)
        print(f"Fixed: {path}")
    else:
        print(f"Search string not found in: {path}")

# 1. Add module docs and ANCHOR tags
fix_file('crates/memfuse-core/src/types.rs',
         'pub mod budget;',
         '//! Core type definitions and re-exports.\n// ANCHOR:DOC:DOC-TYPES-001\n// AGENT:01 STATUS:DONE PRIO:3\n\npub mod budget;')

fix_file('crates/memfuse-core/src/types/domain.rs',
         'use crate::error::{MemFuseError, Result};',
         '//! Domain primitives and core identifiers (DocId, TxId, EntityId).\n// ANCHOR:DOC:DOC-DOMAIN-001\n// AGENT:01 STATUS:DONE PRIO:3\n\nuse crate::error::{MemFuseError, Result};')

fix_file('crates/memfuse-core/src/types/budget.rs',
         'use crate::error::{MemFuseError, Result};',
         '//! Resource budget and memory tracking for LLM operations.\n// ANCHOR:DOC:DOC-BUDGET-001\n// AGENT:01 STATUS:DONE PRIO:3\n\nuse crate::error::{MemFuseError, Result};')

fix_file('crates/memfuse-core/src/types/saos.rs',
         'use super::domain::DocId;',
         '//! Search and Orchestration Schemas (SAOS) for hybrid queries.\n// ANCHOR:DOC:DOC-SAOS-001\n// AGENT:01 STATUS:DONE PRIO:3\n\nuse super::domain::DocId;')

fix_file('crates/memfuse-core/src/types/filter.rs',
         'use serde::{Deserialize, Serialize};',
         '//! Metadata filter expressions for search operations.\n// ANCHOR:DOC:DOC-FILTER-001\n// AGENT:01 STATUS:DONE PRIO:3\n\nuse serde::{Deserialize, Serialize};')

# 2. Add DocId::from_string
fix_file('crates/memfuse-core/src/types/domain.rs',
         '        Ok(Self(u64::from_le_bytes(buf)))\n    }\n}',
         '        Ok(Self(u64::from_le_bytes(buf)))\n    }\n\n    /// Creates a DocId from a string, falling back to DocId(0) on error.\n    /// Used for compatibility with downstream crates.\n    // ANCHOR:FIX:FIX-DOCID-001\n    // AGENT:01 STATUS:DONE PRIO:1\n    pub fn from_string(s: &str) -> Self {\n        Self::try_from_key(s).unwrap_or(Self(0))\n    }\n}')

# 3. Add Graph variant to MemFuseError
fix_file('crates/memfuse-core/src/error.rs',
         '    Text(String),\n}',
         '    Text(String),\n\n    // ═══ Graph Engine ═══\n    // ANCHOR:FIX:FIX-ERROR-001\n    // AGENT:01 STATUS:DONE PRIO:2\n    #[error("Graph error: {0}")]\n    Graph(String),\n}')

# 4. Harden SnapshotRegistry
fix_file('crates/memfuse-core/src/snapshot.rs',
         'use crate::types::TOMBSTONE_BIT;',
         'use crate::error::{MemFuseError, Result};\nuse crate::types::TOMBSTONE_BIT;')

fix_file('crates/memfuse-core/src/snapshot.rs',
         '    pub fn unpin(&self, seq_no: u64) {\n        self.release(seq_no);\n    }',
         '    /// Removes a persistent pin.\n    // ANCHOR:FIX:FIX-SNAPSHOT-001\n    // AGENT:01 STATUS:DONE PRIO:2\n    pub fn unpin(&self, seq_no: u64) {\n        if let Err(e) = self.checked_release(seq_no) {\n            tracing::error!("Failed to unpin sequence number {}: {}", seq_no, e);\n        }\n    }')

fix_file('crates/memfuse-core/src/snapshot.rs',
         '    pub(crate) fn release(&self, seq_no: u64) {\n        let seq_no = seq_no & !TOMBSTONE_BIT;\n        let mut active = self.active.lock();\n        if let Some(count) = active.get_mut(&seq_no) {\n            *count -= 1;\n            if *count == 0 {\n                active.remove(&seq_no);\n            }\n        }\n        self.update_min(&active);\n    }',
         '    pub(crate) fn release(&self, seq_no: u64) {\n        let _ = self.checked_release(seq_no);\n    }\n\n    fn checked_release(&self, seq_no: u64) -> Result<()> {\n        let seq_no = seq_no & !TOMBSTONE_BIT;\n        let mut active = self.active.lock();\n        if let Some(count) = active.get_mut(&seq_no) {\n            *count = count.saturating_sub(1);\n            if *count == 0 {\n                active.remove(&seq_no);\n            }\n            self.update_min(&active);\n            Ok(())\n        } else {\n            Err(MemFuseError::Internal(format!(\n                "Attempted to release non-existent snapshot pin: {}",\n                seq_no\n            )))\n        }\n    }')

# 5. Replace .unwrap() in tests
fix_file('crates/memfuse-core/src/types/budget.rs', 'result.err().unwrap()', 'result.err().expect("test")')
fix_file('crates/memfuse-core/src/types/budget.rs', 'h.join().unwrap()', 'h.join().expect("test")')
fix_file('crates/memfuse-core/src/types/saos.rs', 'query.text_query.unwrap()', 'query.text_query.expect("test")')
fix_file('crates/memfuse-core/src/types/saos.rs', 'query.vector_query.unwrap()', 'query.vector_query.expect("test")')
fix_file('crates/memfuse-core/src/types/saos.rs', '.build().unwrap()', '.build().expect("test")')
fix_file('crates/memfuse-core/src/types/saos.rs', '0.1, 0.1).unwrap()', '0.1, 0.1).expect("test")')
