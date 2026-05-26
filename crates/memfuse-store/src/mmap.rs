//! Hyper-Scale Memory Mapped Indexing (WP-4.1)
//!
//! Maps multi-gigabyte SSTables or Out-of-Core components instantly to RAM without allocations.

// Mmap bindings fundamentally require unsafe memory translations.
#![allow(unsafe_code)]

use memfuse_core::{MemFuseError, Result};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// Safely wraps a `memmap2` slice projection.
pub struct MmapReader {
    mmap: Arc<memmap2::Mmap>,
}

impl MmapReader {
    /// Acquires a safe page mapping from the file at `path`.
    /// Note: This performs a blocking I/O operation and should be called within `spawn_blocking`.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)
            .map_err(|e| MemFuseError::Storage(format!("Failed to open file for mmap: {}", e)))?;

        // ANCHOR:SAFETY:MMAP-002 — Memory Mapping of SSTable file
        // WP:WP-4.1 PRIO:1 NEEDS:NONE
        // AGENT:02 DATE:2026-06-15 STATUS:REVIEW
        // BEGRÜNDUNG: SSTables sind im LSM-Tree unveränderlich. Memory Mapping
        // ermöglicht effizienten Zugriff ohne explizite Syscalls.
        let mmap = unsafe { memmap2::Mmap::map(&file) }
            .map_err(|e| MemFuseError::Storage(format!("Mmap failed: {}", e)))?;

        Ok(Self {
            mmap: Arc::new(mmap),
        })
    }

    /// Access the underlying mmap as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap
    }

    /// Returns the length of the mapping.
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    /// Returns true if the mapping is empty.
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }
}

impl Clone for MmapReader {
    fn clone(&self) -> Self {
        Self {
            mmap: Arc::clone(&self.mmap),
        }
    }
}
