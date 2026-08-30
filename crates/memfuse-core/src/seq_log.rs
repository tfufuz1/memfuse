//! Shared versioned sequence log for snapshot-isolated index searches (`_at` family).

use crate::types::DocId;

/// Versioned sequence log entry for snapshot isolation (`_at` family).
///
/// **Memory Overhead**: Each entry is 24 bytes (8 bytes `DocId` + 8 bytes `insert_seq` + 8 bytes `delete_seq: Option<u64>`).
/// Entries where `delete_seq` is below `min_active_seqno` can be pruned via `compact`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqLogEntry {
    /// Document ID.
    pub doc_id: DocId,
    /// Sequence number at which this document entry was inserted.
    pub insert_seq: u64,
    /// Sequence number at which this document entry was deleted (if deleted).
    pub delete_seq: Option<u64>,
}

/// Helper structure managing sequence log tracking and visibility filtering for index implementations.
#[derive(Debug, Default, Clone)]
pub struct SequenceLog {
    entries: Vec<SeqLogEntry>,
}

impl SequenceLog {
    /// Creates a new empty sequence log.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Records an insert operation at the given sequence number `seq`.
    pub fn record_insert(&mut self, doc_id: DocId, seq: u64) {
        if let Some(entry) = self.entries.iter_mut().rfind(|e| e.doc_id == doc_id) {
            if entry.delete_seq.is_some() {
                self.entries.push(SeqLogEntry {
                    doc_id,
                    insert_seq: seq,
                    delete_seq: None,
                });
            }
        } else {
            self.entries.push(SeqLogEntry {
                doc_id,
                insert_seq: seq,
                delete_seq: None,
            });
        }
    }

    /// Records a delete operation at the given sequence number `seq`.
    pub fn record_delete(&mut self, doc_id: DocId, seq: u64) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .rfind(|e| e.doc_id == doc_id && e.delete_seq.is_none())
        {
            entry.delete_seq = Some(seq);
        }
    }

    /// Checks if `doc_id` was inserted at or before `seq_no` and not deleted at or before `seq_no`.
    pub fn is_visible(&self, doc_id: DocId, seq_no: u64) -> bool {
        let mut inserted = false;
        let mut deleted = false;
        for entry in &self.entries {
            if entry.doc_id == doc_id && entry.insert_seq <= seq_no {
                inserted = true;
                if let Some(del) = entry.delete_seq {
                    deleted = del <= seq_no;
                } else {
                    deleted = false;
                }
            }
        }
        inserted && !deleted
    }

    /// Compacts log entries where deletion sequence number is strictly less than `min_active_seqno`.
    pub fn compact(&mut self, min_active_seqno: u64) {
        self.entries.retain(|entry| {
            if let Some(del_seq) = entry.delete_seq {
                del_seq >= min_active_seqno
            } else {
                true
            }
        });
    }

    /// Returns the number of entries in the sequence log.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the sequence log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_log_insert_delete_visibility() {
        let mut log = SequenceLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);

        let doc1 = DocId::from_key("doc1").expect("valid doc id"); // expect #[cfg(test)]
        let doc2 = DocId::from_key("doc2").expect("valid doc id"); // expect #[cfg(test)]

        // Insert doc1 at seq 10, doc2 at seq 20
        log.record_insert(doc1, 10);
        log.record_insert(doc2, 20);
        assert_eq!(log.len(), 2);
        assert!(!log.is_empty());

        // Visibility before insert
        assert!(!log.is_visible(doc1, 5));
        assert!(!log.is_visible(doc2, 15));

        // Visibility at and after insert
        assert!(log.is_visible(doc1, 10));
        assert!(log.is_visible(doc1, 15));
        assert!(log.is_visible(doc2, 20));

        // Record delete for doc1 at seq 30
        log.record_delete(doc1, 30);
        assert_eq!(log.len(), 2);

        // Visibility around delete seq
        assert!(log.is_visible(doc1, 29));
        assert!(!log.is_visible(doc1, 30));
        assert!(!log.is_visible(doc1, 35));

        // doc2 remains visible
        assert!(log.is_visible(doc2, 35));
    }

    #[test]
    fn test_sequence_log_reinsert_after_delete() {
        let mut log = SequenceLog::new();
        let doc1 = DocId::from_key("doc1").expect("valid doc id"); // expect #[cfg(test)]

        log.record_insert(doc1, 10);
        log.record_delete(doc1, 20);
        assert!(!log.is_visible(doc1, 25));

        // Re-insert doc1 at seq 30
        log.record_insert(doc1, 30);
        assert_eq!(log.len(), 2);

        // Visibility timelines
        assert!(log.is_visible(doc1, 15));
        assert!(!log.is_visible(doc1, 25));
        assert!(log.is_visible(doc1, 30));
        assert!(log.is_visible(doc1, 40));
    }

    #[test]
    fn test_sequence_log_compact() {
        let mut log = SequenceLog::new();
        let doc1 = DocId::from_key("doc1").expect("valid doc id"); // expect #[cfg(test)]
        let doc2 = DocId::from_key("doc2").expect("valid doc id"); // expect #[cfg(test)]

        log.record_insert(doc1, 10);
        log.record_delete(doc1, 20); // deleted at seq 20

        log.record_insert(doc2, 15);
        log.record_delete(doc2, 40); // deleted at seq 40

        assert_eq!(log.len(), 2);

        // Compact with min_active_seqno = 30
        // doc1 (delete_seq = 20 < 30) should be purged.
        // doc2 (delete_seq = 40 >= 30) should be retained.
        log.compact(30);
        assert_eq!(log.len(), 1);

        // Compact with min_active_seqno = 50
        // doc2 (delete_seq = 40 < 50) should be purged.
        log.compact(50);
        assert_eq!(log.len(), 0);
        assert!(log.is_empty());
    }

    #[test]
    fn test_sequence_log_edge_cases() {
        let mut log = SequenceLog::new();
        let doc1 = DocId::from_key("doc1").expect("valid doc id"); // expect #[cfg(test)]

        // Delete non-existent doc should not panic or add entry
        log.record_delete(doc1, 10);
        assert_eq!(log.len(), 0);

        // Multiple deletes on same active insert updates delete_seq or ignores second?
        log.record_insert(doc1, 5);
        log.record_delete(doc1, 15);
        log.record_delete(doc1, 25); // Should not overwrite existing delete_seq if already deleted
        assert!(log.is_visible(doc1, 10));
        assert!(!log.is_visible(doc1, 15));
    }
}
