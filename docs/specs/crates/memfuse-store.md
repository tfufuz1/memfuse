# Agent Specification: memfuse-store

**Agent**: @JULES-02
**Domain**: Storage Engine, LSM Tree, Write-Ahead-Log
**Status**: 🔴 Vulnerable (High Priority)

## Mission Statement
Maintain the indestructible storage foundation of MemFuse. All writes must hit WAL before MemTable. Enforce Zero-Panic strictness across the board.

## Critical Remediation Targets
1. **Remove Unsafe Code**: `src/sstable.rs` uses `#[allow(unsafe_code)]` for `memmap2`. This directly violates the Sovereign Core Doctrine. You MUST strip this out or isolate the `mmap` into a verified secure enclave/`memfuse-core`.
2. **Snapshot Skulls (`pin_checkpoint`, `unpin_checkpoint`)**: Currently placeholders returning `Ok(())`. Implement the reference counting on the SSTables properly.
3. **Compaction Engine (`COMP-001`)**: The background daemon loop is a `TODO`. Wire up the Tokio receiver logic immediately.
4. **Rollback Integrity (`FIND-STO-003`)**: Mitigate the highly destructive `rollback_to_tx` mechanism which incorrectly truncates WAL blindly.
