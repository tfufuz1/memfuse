# Atomic Spec: SSTable Block CRC Validation (FIND-STO-001) - COMPLETED

## Status: ✅ IMPLEMENTED (2026-06-07)

## Context
SSTables in `memfuse-store` previously lacked per-block checksums, making unencrypted SSTables vulnerable to silent data corruption.

## Objective
Implement CRC32 validation for every data block and index block in SSTables.

## Requirements
1. **Per-Block CRC**: Every data block written to disk must be prefixed with a 4-byte CRC32 checksum. - ✅ DONE
2. **Replay Validation**: When reading a block, the CRC is validated. - ✅ DONE
3. **Controlled Failure**: Returns `MemFuseError::ChecksumMismatch` on mismatch. - ✅ DONE
4. **Backward Compatibility**: Uses magic `0x5853464D` ("MFSX") to identify SSTables with CRC support. Legacy "MFST" files remain readable without CRC check. - ✅ DONE

## Implementation Details
### SstableBuilder
- `flush_block`: Computes CRC32 before encryption.
- Format: `[Nonce(12)][Encrypted(CRC(4) + Data...)]`
- `finish`: Adds CRC to Index and Bloom filter blocks.

### SstableReader
- `read_block_at_file`: Decrypts and verifies CRC32.
- `open_with_key_manager`: Verifies CRC for Index and Bloom filter during load.

## Verification
- Test `test_sstable_block_crc_corruption` manually corrupts a bit in an SSTable and verifies that `ChecksumMismatch` is triggered.
- Full `just test` suite passed.
