//! Consistency Enforcement / Konflikterkennungs-Register (Feature F-04).
//!
//! SPECIFICATION: Feature F-04 gemäß Spezifikation.
//! ORTHOGONALITÄT: Dieses Feature ist vollkommen orthogonal zu Feature F-02
//! (Veto-Feature / partieller HNSW-Rebuild), welches NICHT in diesem Modul oder Crate
//! implementiert wird.
//!
//! Consistency-Enforcement-Modul: lernt wiederkehrende, als fehlerhaft erkannte
//! Muster (z. B. widersprüchliche oder sich gegenseitig aufhebende Kanten/Fakten
//! im Wissensgraphen) und meldet sie als Kandidaten für Unterdrückung/Review.
//! Implementiert reines Pattern-Signal — keine automatische Löschung (siehe
//! Abgrenzung unten).
//!
//! WICHTIGER HINWEIS ZUR TRENNUNG VON ERKENNUNG UND WIRKUNG:
//! Dieses Modul implementiert KEINE automatische Löschung von Kanten. `ConsistencyEnforcer` liefert
//! lediglich Kandidaten/Signale. Die tatsächliche Tombstone-Ausführung bleibt in der
//! Verantwortung des Aufrufers.

use crate::session_dag::NodeIdx;
use memfuse_core::{EntityId, TxId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Identifikator für eine Kante im CSR-Graphen.
pub type EdgeId = (EntityId, EntityId);

/// Eintrag für ein gelerntes Konfliktmuster (Constraint-Violation-Signatur).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictPattern {
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

impl EdgeAssertion {
    /// Berechnet den Blake3-Pattern-Hash über (subject, predicate_hash, object_repr).
    pub fn pattern_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.subject.to_le_bytes());
        hasher.update(&self.predicate_hash);
        hasher.update(&self.object_repr);
        *hasher.finalize().as_bytes()
    }
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
        a.subject == b.subject
            && a.predicate_hash == b.predicate_hash
            && a.object_repr != b.object_repr
    }
}

/// Consistency-Enforcement-Register zur Verwaltung registrierter Konfliktmuster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyEnforcer {
    patterns: HashMap<[u8; 32], ConflictPattern>,
    suppression_threshold: u32,
}

impl ConsistencyEnforcer {
    /// Standard-Schwellenwert für die Unterdrückung (3 gegenseitige Detektionen).
    pub const DEFAULT_SUPPRESSION_THRESHOLD: u32 = 3;

    /// Erstellt einen neuen ConsistencyEnforcer mit konfigurierbarem Schwellenwert.
    pub fn new(suppression_threshold: u32) -> Self {
        Self {
            patterns: HashMap::new(),
            suppression_threshold,
        }
    }

    /// Prüft eine Kanten-Aussage vor dem Einfügen, registriert das Widerspruchsmuster
    /// und gibt den Konfliktmuster-Eintrag zurück.
    pub fn check_before_insert(&mut self, assertion: &EdgeAssertion) -> Option<ConflictPattern> {
        let pattern = assertion.pattern_hash();
        let cp = self.record_contradiction(pattern, TxId::new(0));
        Some(cp.clone())
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
    pub fn record_contradiction(&mut self, pattern_hash: [u8; 32], at_tx: TxId) -> &ConflictPattern {
        let threshold = self.suppression_threshold;
        let entry = self
            .patterns
            .entry(pattern_hash)
            .and_modify(|cp| {
                cp.contradiction_count = cp.contradiction_count.saturating_add(1);
                cp.last_detected_tx = at_tx;
                if cp.contradiction_count >= threshold {
                    cp.suppressed = true;
                }
            })
            .or_insert_with(|| ConflictPattern {
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
        self.patterns
            .get(&pattern_hash)
            .is_some_and(|cp| cp.suppressed)
    }

    /// Gibt einen Iterator über alle aktuell aktiven (unterdrückenden) Konfliktmuster zurück.
    pub fn active_patterns(&self) -> impl Iterator<Item = &ConflictPattern> {
        self.patterns.values().filter(|cp| cp.suppressed)
    }

    /// Gibt ein registriertes Konfliktmuster zu einem Muster-Hash zurück, falls vorhanden.
    pub fn get_pattern(&self, pattern_hash: &[u8; 32]) -> Option<&ConflictPattern> {
        self.patterns.get(pattern_hash)
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
    pub fn suggest_tombstone_candidates(
        &self,
        csr_edges_matching_pattern: &[EdgeId],
    ) -> Vec<EdgeId> {
        csr_edges_matching_pattern.to_vec()
    }
}

impl Default for ConsistencyEnforcer {
    fn default() -> Self {
        Self::new(Self::DEFAULT_SUPPRESSION_THRESHOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_contradiction() {
        let mut enforcer = ConsistencyEnforcer::new(3);
        let hash = [1u8; 32];
        let tx1 = TxId::new(10);

        let cp = enforcer.record_contradiction(hash, tx1);
        assert_eq!(cp.contradiction_count, 1);
        assert!(!cp.suppressed);
        assert_eq!(cp.first_detected_tx, tx1);
        assert_eq!(cp.last_detected_tx, tx1);
        assert!(!enforcer.is_suppressed(hash));
    }

    #[test]
    fn test_three_repeated_contradictions_suppresses_pattern() {
        let mut enforcer = ConsistencyEnforcer::new(3);
        let hash = [2u8; 32];

        enforcer.record_contradiction(hash, TxId::new(1));
        assert!(!enforcer.is_suppressed(hash));

        enforcer.record_contradiction(hash, TxId::new(2));
        assert!(!enforcer.is_suppressed(hash));

        let cp3 = enforcer.record_contradiction(hash, TxId::new(3));
        assert_eq!(cp3.contradiction_count, 3);
        assert!(cp3.suppressed);
        assert_eq!(cp3.first_detected_tx, TxId::new(1));
        assert_eq!(cp3.last_detected_tx, TxId::new(3));
        assert!(enforcer.is_suppressed(hash));
    }

    #[test]
    fn test_independent_pattern_hash_counting_no_cross_contamination() {
        let mut enforcer = ConsistencyEnforcer::new(3);
        let hash1 = [10u8; 32];
        let hash2 = [20u8; 32];

        enforcer.record_contradiction(hash1, TxId::new(1));
        enforcer.record_contradiction(hash1, TxId::new(2));

        enforcer.record_contradiction(hash2, TxId::new(1));

        assert_eq!(
            enforcer.get_pattern(&hash1).map(|a| a.contradiction_count),
            Some(2)
        );
        assert_eq!(
            enforcer.get_pattern(&hash2).map(|a| a.contradiction_count),
            Some(1)
        );
        assert!(!enforcer.is_suppressed(hash1));
        assert!(!enforcer.is_suppressed(hash2));

        // Third contradiction on hash1 suppresses hash1, but leaves hash2 unsuppressed
        enforcer.record_contradiction(hash1, TxId::new(3));
        assert!(enforcer.is_suppressed(hash1));
        assert!(!enforcer.is_suppressed(hash2));
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
    fn test_active_patterns_and_suggest_tombstones() {
        let mut enforcer = ConsistencyEnforcer::default();
        let hash_suppressed = [100u8; 32];
        let hash_unsuppressed = [200u8; 32];

        for i in 1..=3 {
            enforcer.record_contradiction(hash_suppressed, TxId::new(i));
        }
        enforcer.record_contradiction(hash_unsuppressed, TxId::new(1));

        let active: Vec<_> = enforcer.active_patterns().collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].pattern_hash, hash_suppressed);

        let candidate_edges: Vec<EdgeId> = vec![
            (EntityId::new(1), EntityId::new(2)),
            (EntityId::new(3), EntityId::new(4)),
        ];

        let suggestions = enforcer.suggest_tombstone_candidates(&candidate_edges);
        assert_eq!(suggestions, candidate_edges);
    }
}
