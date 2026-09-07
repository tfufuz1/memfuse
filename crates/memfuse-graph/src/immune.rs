//! Immunologische Widerspruchsprävention / "Antikörper"-Register (Feature F-04).
//!
//! SPECIFICATION: Feature F-04 gemäß Spezifikation.
//! ORTHOGONALITÄT: Dieses Feature ist vollkommen orthogonal zu Feature F-02
//! (Veto-Feature / partieller HNSW-Rebuild), welches NICHT in diesem Modul oder Crate
//! implementiert wird.
//!
//! Naturvorbild: Ein Immunsystem lernt "Antikörper" gegen wiederkehrende, als fehlerhaft
//! erkannte Muster (z. B. widersprüchliche oder sich gegenseitig aufhebende Kanten/Fakten
//! im Wissensgraphen) und verhindert deren erneute unkritische Aufnahme.
//!
//! WICHTIGER HINWEIS ZUR TRENNUNG VON ERKENNUNG UND WIRKUNG:
//! Dieses Modul implementiert KEINE automatische Löschung von Kanten. `ImmunMemory` liefert
//! lediglich Kandidaten/Signale. Die tatsächliche Tombstone-Ausführung bleibt in der
//! Verantwortung des Aufrufers.

use crate::session_dag::NodeIdx;
use memfuse_core::{EntityId, TxId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Identifikator für eine Kante im CSR-Graphen.
pub type EdgeId = (EntityId, EntityId);

/// Antikörper-Eintrag für ein gelerntes Widerspruchsmuster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Antibody {
    /// Blake3-Hash über die normalisierte (subject, predicate, object)-Tripel-Signatur der widersprüchlichen Aussage.
    pub pattern_hash: [u8; 32],
    /// Anzahl der bisher detektierten Widersprüche für dieses Muster.
    pub contradiction_count: u32,
    /// Transaktions-ID der ersten Detektion.
    pub first_detected_tx: TxId,
    /// Transaktions-ID der aktuellsten Detektion.
    pub last_detected_tx: TxId,
    /// Status, ob das Muster aufgrund erreichter Schwelle unterdrückt wird.
    pub suppressed: bool,
}

/// Abstrakte Aussage über eine Kante für die semantische Widerspruchsprävention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeAssertion {
    /// Subjekt-Knoten der Aussage.
    pub subject: NodeIdx,
    /// Hash des Prädikats/Relationsnamens.
    pub predicate_hash: [u8; 32],
    /// Repräsentation des Objekts/Zielknotens oder Werts.
    pub object_repr: Vec<u8>,
}

/// Trait für modulare Widerspruchserkennungs-Strategien.
pub trait ContradictionDetector {
    /// Prüft, ob zwei Aussagen im Widerspruch zueinander stehen.
    fn conflicts(&self, a: &EdgeAssertion, b: &EdgeAssertion) -> bool;
}

/// Referenzimplementierung für exakten Prädikats-Konflikt:
/// Ein Widerspruch liegt vor, wenn Subjekt und Prädikats-Hash identisch sind,
/// die Objekt-Repräsentation jedoch unterschiedlich ist.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExactPredicateConflictDetector;

impl ContradictionDetector for ExactPredicateConflictDetector {
    fn conflicts(&self, a: &EdgeAssertion, b: &EdgeAssertion) -> bool {
        a.subject == b.subject && a.predicate_hash == b.predicate_hash && a.object_repr != b.object_repr
    }
}

/// Immunologisches Gedächtnis zur Verwaltung registrierter Antikörper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmunMemory {
    antibodies: HashMap<[u8; 32], Antibody>,
    suppression_threshold: u32,
}

impl ImmunMemory {
    /// Standard-Schwellenwert für die Unterdrückung (3 gegenseitige Detektionen).
    pub const DEFAULT_SUPPRESSION_THRESHOLD: u32 = 3;

    /// Erstellt ein neues immunologisches Gedächtnis mit konfigurierbarem Schwellenwert.
    pub fn new(suppression_threshold: u32) -> Self {
        Self {
            antibodies: HashMap::new(),
            suppression_threshold,
        }
    }

    /// Bewertet mit Hilfe des übergebenen [`ContradictionDetector`]s, ob zwei Kanten-Aussagen
    /// einen semantischen Widerspruch darstellen.
    pub fn detect_contradiction<D: ContradictionDetector>(
        &self,
        detector: &D,
        a: &EdgeAssertion,
        b: &EdgeAssertion,
    ) -> bool {
        detector.conflicts(a, b)
    }

    /// Registriert oder aktualisiert einen Widerspruch für den gegebenen Muster-Hash (`pattern_hash`).
    ///
    /// Erhöht `contradiction_count` und setzt `suppressed = true`, sobald der Zähler
    /// den `suppression_threshold` erreicht oder überschreitet.
    pub fn record_contradiction(&mut self, pattern_hash: [u8; 32], at_tx: TxId) -> &Antibody {
        let threshold = self.suppression_threshold;
        let entry = self
            .antibodies
            .entry(pattern_hash)
            .and_modify(|ab| {
                ab.contradiction_count = ab.contradiction_count.saturating_add(1);
                ab.last_detected_tx = at_tx;
                if ab.contradiction_count >= threshold {
                    ab.suppressed = true;
                }
            })
            .or_insert_with(|| Antibody {
                pattern_hash,
                contradiction_count: 1,
                first_detected_tx: at_tx,
                last_detected_tx: at_tx,
                suppressed: 1 >= threshold,
            });
        entry
    }

    /// Prüft, ob ein gegebenes Muster unterdrückt wird (`suppressed == true`).
    pub fn is_suppressed(&self, pattern_hash: [u8; 32]) -> bool {
        self.antibodies
            .get(&pattern_hash)
            .map_or(false, |ab| ab.suppressed)
    }

    /// Gibt einen Iterator über alle aktuell aktiven (unterdrückenden) Antikörper zurück.
    pub fn active_antibodies(&self) -> impl Iterator<Item = &Antibody> {
        self.antibodies.values().filter(|ab| ab.suppressed)
    }

    /// Gibt einen registrierten Antikörper zu einem Muster-Hash zurück, falls vorhanden.
    pub fn get_antibody(&self, pattern_hash: &[u8; 32]) -> Option<&Antibody> {
        self.antibodies.get(pattern_hash)
    }

    /// Gibt die konfigurierte Unterdrückungsschwelle zurück.
    pub fn suppression_threshold(&self) -> u32 {
        self.suppression_threshold
    }

    /// Integration mit der Kanten-Provenienz-Cascade (§4.5 der Spezifikation).
    ///
    /// Nimmt eine Liste von Kanten-IDs entgegen, die einem unterdrückten Muster entsprechen,
    /// und gibt diese als Kandidaten für die Tombstone-Markierung durch den Aufrufer zurück.
    ///
    /// HINWEIS: Dies ist eine Passthrough-Funktion. Die Ausführung des Tombstoning (z. B. via
    /// `CsrGraph::remove_edge()`) obliegt ausschließlich dem Aufrufer (Trennung von Erkennung und Wirkung).
    pub fn suggest_tombstone_candidates(&self, csr_edges_matching_pattern: &[EdgeId]) -> Vec<EdgeId> {
        csr_edges_matching_pattern.to_vec()
    }
}

impl Default for ImmunMemory {
    fn default() -> Self {
        Self::new(Self::DEFAULT_SUPPRESSION_THRESHOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_contradiction() {
        let mut mem = ImmunMemory::new(3);
        let hash = [1u8; 32];
        let tx1 = TxId::new(10);

        let ab = mem.record_contradiction(hash, tx1);
        assert_eq!(ab.contradiction_count, 1);
        assert!(!ab.suppressed);
        assert_eq!(ab.first_detected_tx, tx1);
        assert_eq!(ab.last_detected_tx, tx1);
        assert!(!mem.is_suppressed(hash));
    }

    #[test]
    fn test_three_repeated_contradictions_suppresses_pattern() {
        let mut mem = ImmunMemory::new(3);
        let hash = [2u8; 32];

        mem.record_contradiction(hash, TxId::new(1));
        assert!(!mem.is_suppressed(hash));

        mem.record_contradiction(hash, TxId::new(2));
        assert!(!mem.is_suppressed(hash));

        let ab3 = mem.record_contradiction(hash, TxId::new(3));
        assert_eq!(ab3.contradiction_count, 3);
        assert!(ab3.suppressed);
        assert_eq!(ab3.first_detected_tx, TxId::new(1));
        assert_eq!(ab3.last_detected_tx, TxId::new(3));
        assert!(mem.is_suppressed(hash));
    }

    #[test]
    fn test_independent_pattern_hash_counting_no_cross_contamination() {
        let mut mem = ImmunMemory::new(3);
        let hash1 = [10u8; 32];
        let hash2 = [20u8; 32];

        mem.record_contradiction(hash1, TxId::new(1));
        mem.record_contradiction(hash1, TxId::new(2));

        mem.record_contradiction(hash2, TxId::new(1));

        assert_eq!(mem.get_antibody(&hash1).map(|a| a.contradiction_count), Some(2));
        assert_eq!(mem.get_antibody(&hash2).map(|a| a.contradiction_count), Some(1));
        assert!(!mem.is_suppressed(hash1));
        assert!(!mem.is_suppressed(hash2));

        // Third contradiction on hash1 suppresses hash1, but leaves hash2 unsuppressed
        mem.record_contradiction(hash1, TxId::new(3));
        assert!(mem.is_suppressed(hash1));
        assert!(!mem.is_suppressed(hash2));
    }

    #[test]
    fn test_exact_predicate_conflict_detector() {
        let detector = ExactPredicateConflictDetector;

        let a = EdgeAssertion {
            subject: 42,
            predicate_hash: [5u8; 32],
            object_repr: b"Berlin".to_vec(),
        };

        let b_conflict = EdgeAssertion {
            subject: 42,
            predicate_hash: [5u8; 32],
            object_repr: b"Munich".to_vec(),
        };

        let b_identical = EdgeAssertion {
            subject: 42,
            predicate_hash: [5u8; 32],
            object_repr: b"Berlin".to_vec(),
        };

        let b_different_subject = EdgeAssertion {
            subject: 99,
            predicate_hash: [5u8; 32],
            object_repr: b"Munich".to_vec(),
        };

        let b_different_predicate = EdgeAssertion {
            subject: 42,
            predicate_hash: [6u8; 32],
            object_repr: b"Munich".to_vec(),
        };

        // Same (subject, predicate_hash), different object_repr -> CONFLICT
        assert!(detector.conflicts(&a, &b_conflict));

        // Identical triple -> NO conflict
        assert!(!detector.conflicts(&a, &b_identical));

        // Different subject or predicate -> NO conflict
        assert!(!detector.conflicts(&a, &b_different_subject));
        assert!(!detector.conflicts(&a, &b_different_predicate));
    }

    #[test]
    fn test_active_antibodies_and_suggest_tombstones() {
        let mut mem = ImmunMemory::default();
        let hash_suppressed = [100u8; 32];
        let hash_unsuppressed = [200u8; 32];

        for i in 1..=3 {
            mem.record_contradiction(hash_suppressed, TxId::new(i));
        }
        mem.record_contradiction(hash_unsuppressed, TxId::new(1));

        let active: Vec<_> = mem.active_antibodies().collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].pattern_hash, hash_suppressed);

        let candidate_edges: Vec<EdgeId> = vec![
            (EntityId::new(1), EntityId::new(2)),
            (EntityId::new(3), EntityId::new(4)),
        ];

        let suggestions = mem.suggest_tombstone_candidates(&candidate_edges);
        assert_eq!(suggestions, candidate_edges);
    }
}
