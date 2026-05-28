# Atomic Spec: FIND-STO-003 Rollback SSTables

## 1. Problem Statement
The `rollback_to_tx` operation in `LsmStorage` only clears MemTables and truncates the WAL. It ignores SSTables that might contain data from transactions newer than the target transaction ID. This leads to data inconsistency after a rollback.

## 2. Proposed Solution
1. Update `SstableMetadata` to include `min_tx_id` and `max_tx_id`.
2. Update `SstableBuilder` to track the `min`/`max` transaction IDs of all entries added to the SSTable.
3. Update `SstableReader` to expose these transaction IDs.
4. Modify `LsmStorage::rollback_to_tx` to:
   - Identify SSTables where `min_tx_id > target_tx` and remove them (both from memory and disk).
   - Identify SSTables where `min_tx_id <= target_tx < max_tx_id`. These contain "mixed" data. 
     - *Simplification for now*: If an SSTable is mixed, we might need to "compact" it or just ensure reads filter out entries > `target_tx`. 
     - *Better approach*: Since `rollback_to_tx` is a destructive operation, we should probably just treat these mixed SSTables as invalid and remove them, relying on the WAL to reconstruct the valid parts into the MemTable. Wait, if the data is in an SSTable, it's NOT in the WAL (WAL was flushed). 
     - *Correction*: When a MemTable is flushed to an SSTable, the WAL is rotated. So the old WAL might have been deleted. 
     - *Refined Plan*: 
       - If `min_tx_id > target_tx`: Delete SSTable.
       - If `max_tx_id <= target_tx`: Keep SSTable.
       - If `min_tx_id <= target_tx < max_tx_id`: This case should be rare if we flush regularly, but if it happens, we must either rewrite the SSTable or ensure the reader respects the `target_tx`. 
       - Actually, for a pure "Rollback", deleting anything > `target_tx` is correct. If an SSTable has some valid and some invalid data, we MUST filter.

## 3. Technical Changes
- `SstableMetadata` struct changes.
- `SstableBuilder::add` should probably take a `seq_no` or `tx_id`. 
- `LsmStorage::flush` must pass the transaction range to the builder.
- `LsmStorage::rollback_to_tx` logic to filter `self.sstables`.

## 4. Verification Plan
- `test_rollback_with_sstables`:
  1. Insert data for TX 1, TX 2.
  2. Flush (SSTable 1 contains TX 1, 2).
  3. Insert data for TX 3, TX 4.
  4. Flush (SSTable 2 contains TX 3, 4).
  5. Rollback to TX 2.
  6. Verify SSTable 2 is gone.
  7. Verify SSTable 1 is still there and only TX 1, 2 data is visible.
