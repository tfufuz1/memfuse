//! F-03: EdgeReinforcementBuffer — Akkumuliert Co-Occurrence- und Traversal-
//! Update-Signale für asynchronen, Scheduler-gesteuerten Flush in den CSR-Graphen.
//!
//! FEATURE-FLAG: `edge-reinforcement-learning`
//! PARALLELITÄT: Thread-sicher via `parking_lot::Mutex` (keine Holds über .await).
//! FLUSH-STRATEGIE: Alle gepufferten Updates werden in einem Lock-Batch auf den
//!   CSR-Graphen angewendet, dann gecleart. Partial-Flush ist nicht unterstützt.

#[cfg(feature = "edge-reinforcement-learning")]
pub mod edge_reinforcement_buffer {
    use crate::csr::CsrGraph;
    use crate::edge_reinforcement::EdgeReinforcementConfig;
    use memfuse_core::EntityId;
    use parking_lot::Mutex;

    /// Ein gepufferter Co-Occurrence-Update: zwei ko-aktivierte Entitäten.
    #[derive(Debug, Clone)]
    pub struct CooccurrenceSignal {
        pub from: EntityId,
        pub to: EntityId,
        pub co_activation: f32,
    }

    /// Ein gepufferter Traversal-Update: eine traversierte Kante mit Pfadlänge.
    #[derive(Debug, Clone)]
    pub struct TraversalSignal {
        pub from: EntityId,
        pub to: EntityId,
        pub path_length: usize,
    }

    /// Puffert pending Edge-Reinforcement-Updates bis zum nächsten Scheduler-Flush.
    ///
    /// Aufrufer (z.B. Collection::search nach erfolgreichem Retrieval) akkumulieren
    /// Signale via `push_*`. Der `MaintenanceScheduler` leert den Buffer periodisch
    /// via `flush_to_graph`.
    #[derive(Debug, Default)]
    pub struct EdgeReinforcementBuffer {
        cooccurrence: Mutex<Vec<CooccurrenceSignal>>,
        traversal: Mutex<Vec<TraversalSignal>>,
    }

    impl EdgeReinforcementBuffer {
        pub fn new() -> Self {
            Self::default()
        }

        /// Fügt ein Co-Occurrence-Signal hinzu (ko-aktivierte Entitäten in derselben
        /// Retrieval-Session). Kann von mehreren Threads gleichzeitig aufgerufen werden.
        pub fn push_cooccurrence(&self, from: EntityId, to: EntityId, co_activation: f32) {
            self.cooccurrence.lock().push(CooccurrenceSignal { from, to, co_activation });
        }

        /// Fügt ein Traversal-Signal hinzu (traversierte Kante mit Pfadlänge).
        pub fn push_traversal(&self, from: EntityId, to: EntityId, path_length: usize) {
            self.traversal.lock().push(TraversalSignal { from, to, path_length });
        }

        /// Gibt die Anzahl der gepufferten Co-Occurrence-Signale zurück (für Monitoring).
        pub fn cooccurrence_count(&self) -> usize {
            self.cooccurrence.lock().len()
        }

        /// Gibt die Anzahl der gepufferten Traversal-Signale zurück (für Monitoring).
        pub fn traversal_count(&self) -> usize {
            self.traversal.lock().len()
        }

        /// Wendet alle gepufferten Signale auf den CSR-Graphen an und leert den Buffer.
        ///
        /// Wird ausschließlich vom `MaintenanceScheduler::run_tick()` aufgerufen.
        /// Nach Abschluss sind `cooccurrence` und `traversal` leer.
        pub fn flush_to_graph(&self, graph: &CsrGraph, config: &EdgeReinforcementConfig) {
            use crate::edge_reinforcement::{
                apply_cooccurrence_reinforcement, apply_traversal_reinforcement,
                apply_weight_normalization,
            };

            let cooccurrence_signals = {
                let mut guard = self.cooccurrence.lock();
                std::mem::take(&mut *guard)
            };
            let traversal_signals = {
                let mut guard = self.traversal.lock();
                std::mem::take(&mut *guard)
            };

            // Wende Co-Occurrence-Updates an
            let mut inner = graph.inner_write();
            for signal in &cooccurrence_signals {
                if let Some(edge) = inner.find_edge_mut_by_entities(signal.from, signal.to) {
                    apply_cooccurrence_reinforcement(edge, signal.co_activation, config);
                }
            }

            // Wende Traversal-Updates an
            for signal in &traversal_signals {
                if let Some(edge) = inner.find_edge_mut_by_entities(signal.from, signal.to) {
                    apply_traversal_reinforcement(edge, signal.path_length, config);
                }
            }

            // Normalisierungs-Pass: Halte Gesamt-Gewicht pro Knoten unter W_max
            for entity_id in inner.all_entity_ids() {
                let outgoing = inner.outgoing_edges_mut(entity_id);
                apply_weight_normalization(outgoing, config.w_max);
            }

            inner.sync_edge_reinforcement_weights(config);

            tracing::debug!(
                cooccurrence_applied = cooccurrence_signals.len(),
                traversal_applied = traversal_signals.len(),
                "F-03 EdgeReinforcementBuffer: flush_to_graph abgeschlossen"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::edge_reinforcement_buffer::*;
    use memfuse_core::EntityId;

    #[test]
    fn test_push_and_count() {
        let buf = EdgeReinforcementBuffer::new();
        buf.push_cooccurrence(EntityId::new(1), EntityId::new(2), 0.8);
        buf.push_traversal(EntityId::new(1), EntityId::new(3), 2);
        assert_eq!(buf.cooccurrence_count(), 1);
        assert_eq!(buf.traversal_count(), 1);
    }

    #[test]
    fn test_flush_empties_buffer() {
        // Smoke-Test: flush auf einem leeren CsrGraph darf nicht paniken.
        use crate::csr::CsrGraph;
        use crate::edge_reinforcement::EdgeReinforcementConfig;
        let buf = EdgeReinforcementBuffer::new();
        buf.push_cooccurrence(EntityId::new(1), EntityId::new(2), 0.5);
        let graph = CsrGraph::new();
        let config = EdgeReinforcementConfig::default();
        buf.flush_to_graph(&graph, &config); // kein Panic, keine Assertion
        assert_eq!(buf.cooccurrence_count(), 0); // Buffer geleert
    }
}
