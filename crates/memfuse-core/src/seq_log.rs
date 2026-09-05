//! Shared versioned sequence log for snapshot-isolated index searches (`_at` family).

// FILE-CONTEXT
// STAND: 2026-08-30T18:51:56Z (SESSION: e459bd5f)
// ZWECK: Shared Versioned Sequence Log für Snapshot-isolierte Index-Suchen (_at Familie).
// INVARIANTEN: Sichtbarkeitsprüfung: insert_seq <= as_of && (delete_seq.is_none() || delete_seq > as_of).
// HOTSPOTS: 20-75
// NICHT-OFFENSICHTLICH: Compaction prunt Einträge erst wenn delete_seq < min_active_seqno.
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md (ADR-024)

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

/// Represents a historical sequence log change (insert or delete) for delta replaying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeqLogChange {
    /// A document insertion at a specific sequence number.
    Insert {
        /// Document ID.
        doc_id: DocId,
        /// Sequence number of the insert.
        seq: u64,
    },
    /// A document deletion at a specific sequence number.
    Delete {
        /// Document ID.
        doc_id: DocId,
        /// Sequence number of the delete.
        seq: u64,
    },
}

impl SeqLogChange {
    /// Returns the sequence number associated with this change.
    pub fn seq(&self) -> u64 {
        match self {
            Self::Insert { seq, .. } | Self::Delete { seq, .. } => *seq,
        }
    }

    /// Returns the document ID associated with this change.
    pub fn doc_id(&self) -> DocId {
        match self {
            Self::Insert { doc_id, .. } | Self::Delete { doc_id, .. } => *doc_id,
        }
    }
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

    /// Returns all sequence log changes that occurred strictly after `snapshot_seq`,
    /// ordered chronologically by sequence number.
    pub fn changes_since(&self, snapshot_seq: u64) -> Vec<SeqLogChange> {
        let mut changes = Vec::new();
        for entry in &self.entries {
            if entry.insert_seq > snapshot_seq {
                changes.push(SeqLogChange::Insert {
                    doc_id: entry.doc_id,
                    seq: entry.insert_seq,
                });
            }
            if let Some(del_seq) = entry.delete_seq {
                if del_seq > snapshot_seq {
                    changes.push(SeqLogChange::Delete {
                        doc_id: entry.doc_id,
                        seq: del_seq,
                    });
                }
            }
        }
        changes.sort_by_key(|c| c.seq());
        changes
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

    #[test]
    fn test_sequence_log_changes_since() {
        let mut log = SequenceLog::new();
        let doc1 = DocId::from_key("doc1").expect("valid doc id");
        let doc2 = DocId::from_key("doc2").expect("valid doc id");

        log.record_insert(doc1, 10);
        log.record_insert(doc2, 20);
        log.record_delete(doc1, 30);
        log.record_insert(doc1, 40);

        let changes_0 = log.changes_since(0);
        assert_eq!(
            changes_0,
            vec![
                SeqLogChange::Insert {
                    doc_id: doc1,
                    seq: 10
                },
                SeqLogChange::Insert {
                    doc_id: doc2,
                    seq: 20
                },
                SeqLogChange::Delete {
                    doc_id: doc1,
                    seq: 30
                },
                SeqLogChange::Insert {
                    doc_id: doc1,
                    seq: 40
                },
            ]
        );

        let changes_15 = log.changes_since(15);
        assert_eq!(
            changes_15,
            vec![
                SeqLogChange::Insert {
                    doc_id: doc2,
                    seq: 20
                },
                SeqLogChange::Delete {
                    doc_id: doc1,
                    seq: 30
                },
                SeqLogChange::Insert {
                    doc_id: doc1,
                    seq: 40
                },
            ]
        );

        let changes_35 = log.changes_since(35);
        assert_eq!(
            changes_35,
            vec![SeqLogChange::Insert {
                doc_id: doc1,
                seq: 40
            }]
        );

        let changes_50 = log.changes_since(50);
        assert!(changes_50.is_empty());
    }
}
