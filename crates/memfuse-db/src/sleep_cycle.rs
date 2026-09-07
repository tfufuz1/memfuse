// FILE-CONTEXT
// ZWECK: Sleep-Cycle-Architecture - NREM-Phase (Non-REM: Strukturierte Gedächtniskonsolidierung, Near-Duplicate-Detection & Segmentation).
// INVARIANTEN: Strikte Trennung von NREM (strukturierte statische/statistische Konsolidierung) und REM (generative Wissenssynthese).
//              Keine LLM-API-Aufrufe in der NREM-Phase. Keine Abhängigkeit zu memfuse-graph (P1-DAG-Integrität).
// STAND: TS:2026-08-29T18:00:00Z

//! NREM Phase (Non-Rapid Eye Movement) Memory Consolidation.
//!
//! # Architektur-Hinweis (REM vs. NREM)
//! Die REM-Phase (generative Wissenssynthese via LLM) ist **NICHT** Teil dieses Moduls
//! und wird in einer separaten Komponente/Prompt implementiert.
//! Dieses Modul deckt ausschließlich die NREM-Phase ab:
//! - Sequenzielles Sliding-Window-Clustering zeitlich benachbarter Turn-Embeddings.
//! - Segmentlokale Near-Duplicate-Detection (O(n²) nur innerhalb eines Segments).
//! - Identifikation verwaister Graph-Kanten zur kaskadierenden Bereinigung.

use crate::context_compaction::{CompactedContext, ContextCompactor};
use memfuse_core::{ContextChunk, DocId};
use std::collections::HashSet;

/// Konfiguration für die NREM-Konsolidierungsphase.
#[derive(Debug, Clone)]
pub struct NremConfig {
    /// Mindestanzahl von Turns pro Segment (Default: 3).
    pub min_turns_per_segment: usize,
    /// Maximale Anzahl von Turns pro Segment (Default: 20).
    pub max_turns_per_segment: usize,
    /// Cosine-Similarity-Schwellwert für Near-Duplicate-Detection (Default: 0.95).
    pub near_duplicate_cosine_threshold: f32,
}

impl Default for NremConfig {
    fn default() -> Self {
        Self {
            min_turns_per_segment: 3,
            max_turns_per_segment: 20,
            near_duplicate_cosine_threshold: 0.95,
        }
    }
}

/// Repräsentiert ein semantisch zusammenhängendes Segment aus aufeinanderfolgenden Turns.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnSegment {
    /// Liste aller DocIds der Turns in diesem Segment.
    pub turn_ids: Vec<DocId>,
    /// Repräsentatives Embedding des Segments.
    /// Berechnet als Zentroid (Mittelwertsvektor) aller Turn-Embeddings des Segments,
    /// um die semantische Mitte des Segments stabil abzubilden.
    pub representative_embedding: Vec<f32>,
}

/// Ergebnis der NREM-Konsolidierungsphase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NremPhaseResult {
    /// Anzahl der erzeugten Segmente.
    pub segments_created: usize,
    /// Liste aller DocIds, die als Duplikate markiert/tombstoned wurden.
    pub duplicates_tombstoned: Vec<DocId>,
    /// Chunks, für die abhängige Graph-Kanten via Kaskadierungslogik nachgezogen werden müssen.
    /// Zur Einhaltung der P1-DAG-Integrität liefert dieses Modul NUR die Liste und ruft `memfuse-graph`
    /// nicht selbst auf, um Crate-Zyklen zu vermeiden.
    pub cascade_edge_tombstones_needed: Vec<DocId>,
}

/// Berechnet die Cosine-Similarity zwischen zwei Vektoren ohne `panic!` oder `unwrap()`.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a <= 0.0 || norm_b <= 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

#[derive(Debug, Clone)]
struct WorkingSegment {
    turns: Vec<(DocId, Vec<f32>)>,
    representative: Vec<f32>,
}

impl WorkingSegment {
    fn new(first_doc_id: DocId, first_emb: Vec<f32>) -> Self {
        Self {
            representative: first_emb.clone(),
            turns: vec![(first_doc_id, first_emb)],
        }
    }

    fn add_turn(&mut self, doc_id: DocId, emb: Vec<f32>) {
        self.turns.push((doc_id, emb));
        let dim = self.representative.len();
        if dim == 0 {
            return;
        }
        let mut sum = vec![0.0f32; dim];
        for (_, turn_emb) in &self.turns {
            for (s, v) in sum.iter_mut().zip(turn_emb.iter()) {
                *s += *v;
            }
        }
        let count = self.turns.len() as f32;
        for s in sum.iter_mut() {
            *s /= count;
        }
        self.representative = sum;
    }
}

/// Gruppiert semantisch zusammenhängende, zeitlich benachbarte Turns via sequenziellem Sliding-Window-Clustering.
///
/// Ein neuer Turn gehört zum aktuellen Segment, wenn seine Cosine-Similarity zum Segment-Repräsentanten
/// über dem Schwellwert liegt UND `max_turns_per_segment` nicht überschritten ist.
///
/// **Sonderregel zur Segment-Kohäsion:**
/// Ein Segment unter `min_turns_per_segment` wird NICHT isoliert als Mikro-Segment belassen,
/// sondern mit dem Nachbarsegment zusammengeführt (vorrangig mit dem vorausgehenden, andernfalls mit dem nachfolgenden).
pub fn group_turns_into_segments(
    turns: &[(DocId, Vec<f32>)],
    config: &NremConfig,
) -> Vec<TurnSegment> {
    if turns.is_empty() {
        return Vec::new();
    }

    let mut raw_segments: Vec<WorkingSegment> = Vec::new();
    let mut current_segment: Option<WorkingSegment> = None;

    for (doc_id, emb) in turns {
        match current_segment.as_mut() {
            Some(seg) => {
                let sim = cosine_similarity(&seg.representative, emb);
                // Kohäsions-Check: hohe Ähnlichkeit und Kapazität vorhanden
                if sim >= config.near_duplicate_cosine_threshold
                    && seg.turns.len() < config.max_turns_per_segment
                {
                    seg.add_turn(*doc_id, emb.clone());
                } else {
                    if let Some(seg) = current_segment.take() {
                        raw_segments.push(seg);
                    }
                    current_segment = Some(WorkingSegment::new(*doc_id, emb.clone()));
                }
            }
            None => {
                current_segment = Some(WorkingSegment::new(*doc_id, emb.clone()));
            }
        }
    }

    if let Some(seg) = current_segment {
        raw_segments.push(seg);
    }

    if raw_segments.is_empty() {
        return Vec::new();
    }

    // Merging-Pass für Mikro-Segmente unter min_turns_per_segment
    let mut merged: Vec<WorkingSegment> = Vec::new();

    for seg in raw_segments {
        if let Some(last) = merged.last_mut() {
            if last.turns.len() < config.min_turns_per_segment {
                // Letztes Segment ist zu klein -> verschmelze aktuelles Segment hinein
                for (id, emb) in seg.turns {
                    last.add_turn(id, emb);
                }
                continue;
            }
        }
        merged.push(seg);
    }

    // Prüfe abschließend das letzte Segment in merged
    if merged.len() > 1 {
        let last_idx = merged.len() - 1;
        if merged[last_idx].turns.len() < config.min_turns_per_segment {
            let last_seg = merged.remove(last_idx);
            let prev = &mut merged[last_idx - 1];
            for (id, emb) in last_seg.turns {
                prev.add_turn(id, emb);
            }
        }
    }

    merged
        .into_iter()
        .map(|ws| TurnSegment {
            turn_ids: ws.turns.into_iter().map(|(id, _)| id).collect(),
            representative_embedding: ws.representative,
        })
        .collect()
}

/// Führt einen paarweisen Cosine-Similarity-Vergleich INNERHALB eines Segments durch (O(n²) segmentlokal).
///
/// Bei `similarity > threshold` wird der ÄLTERE Turn (kleinere `DocId` als Proxy für frühere Erstellung)
/// als Duplikat markiert.
///
/// TODO: Falls `DocId` in zukünftigen Speichermodellen nicht streng monoton mit der Erstellungszeit korreliert,
/// sollte diese Funktion `TxId` oder explizite Timestamps als Parameter anstelle von `DocId` akzeptieren.
///
/// RÜCKGABE: `Vec<(DocId /* zu tombstonen: älterer Turn */, DocId /* Original: neuerer/wichtigerer Turn */)>`
pub fn detect_near_duplicates(
    turns: &[(DocId, Vec<f32>)],
    threshold: f32,
) -> Vec<(DocId, DocId)> {
    let mut pairs = Vec::new();
    let n = turns.len();

    for i in 0..n {
        for j in (i + 1)..n {
            let (doc_id_i, emb_i) = &turns[i];
            let (doc_id_j, emb_j) = &turns[j];

            if doc_id_i == doc_id_j {
                continue;
            }

            let sim = cosine_similarity(emb_i, emb_j);
            if sim > threshold {
                let (older, newer) = if doc_id_i.inner() < doc_id_j.inner() {
                    (*doc_id_i, *doc_id_j)
                } else {
                    (*doc_id_j, *doc_id_i)
                };
                pairs.push((older, newer));
            }
        }
    }

    pairs
}

/// Orchestriert die NREM-Phase (Segmentierung & Near-Duplicate-Detection).
///
/// Führt KEINE LLM-API-Aufrufe durch (NREM ist rein strukturell/statistisch).
pub fn run_nrem_phase(turns: &[(DocId, Vec<f32>)], config: &NremConfig) -> NremPhaseResult {
    if turns.is_empty() {
        return NremPhaseResult {
            segments_created: 0,
            duplicates_tombstoned: Vec::new(),
            cascade_edge_tombstones_needed: Vec::new(),
        };
    }

    let segments = group_turns_into_segments(turns, config);
    let mut duplicates = Vec::new();

    // Map von DocId -> Vec<f32> für schnellen Zugriff per Segment
    let turn_map: std::collections::HashMap<DocId, Vec<f32>> =
        turns.iter().cloned().collect();

    for segment in &segments {
        let segment_turns: Vec<(DocId, Vec<f32>)> = segment
            .turn_ids
            .iter()
            .filter_map(|id| turn_map.get(id).map(|emb| (*id, emb.clone())))
            .collect();

        let dup_pairs = detect_near_duplicates(&segment_turns, config.near_duplicate_cosine_threshold);
        for (older, _newer) in dup_pairs {
            duplicates.push(older);
        }
    }

    duplicates.sort_unstable_by_key(|d| d.inner());
    duplicates.dedup();

    let cascade_edge_tombstones_needed = duplicates.clone();

    NremPhaseResult {
        segments_created: segments.len(),
        duplicates_tombstoned: duplicates,
        cascade_edge_tombstones_needed,
    }
}

/// Adapterfunktion zur Kompaktierung eines Segments via des bereits vorhandenen `ContextCompactor`.
///
/// Wählt Chunks aus `chunks`, die zu `segment.turn_ids` gehören, und führt `ContextCompactor::compact` aus.
pub fn compact_segment_via_context_compactor(
    segment: &TurnSegment,
    compactor: &ContextCompactor,
    chunks: &[ContextChunk],
) -> CompactedContext {
    let turn_set: HashSet<DocId> = segment.turn_ids.iter().copied().collect();
    let segment_chunks: Vec<ContextChunk> = chunks
        .iter()
        .filter(|c| turn_set.contains(&c.doc_id))
        .cloned()
        .collect();

    compactor.compact(segment_chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_embedding(base: f32, dim: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; dim];
        if dim > 0 {
            v[0] = base;
            for i in 1..dim {
                v[i] = 0.1 * (i as f32);
            }
        }
        // Normalize
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
        v
    }

    #[test]
    fn test_group_turns_into_segments_two_clusters() {
        // 10 synthetic embeddings: 5 in Cluster A, 5 in Cluster B
        let mut turns = Vec::new();
        // Cluster A (orthogonal to B)
        let emb_a = vec![1.0, 0.0, 0.0, 0.0];
        for i in 1..=5 {
            turns.push((DocId::new(i), emb_a.clone()));
        }
        // Cluster B
        let emb_b = vec![0.0, 1.0, 0.0, 0.0];
        for i in 6..=10 {
            turns.push((DocId::new(i), emb_b.clone()));
        }

        let config = NremConfig {
            min_turns_per_segment: 3,
            max_turns_per_segment: 20,
            near_duplicate_cosine_threshold: 0.95,
        };

        let segments = group_turns_into_segments(&turns, &config);
        assert_eq!(segments.len(), 2, "10 turns with 2 distinct clusters must produce exactly 2 segments");
        assert_eq!(segments[0].turn_ids.len(), 5);
        assert_eq!(segments[1].turn_ids.len(), 5);
    }

    #[test]
    fn test_detect_near_duplicates_older_tombstoned() {
        let emb = vec![1.0, 0.0, 0.0, 0.0];
        // DocId 10 is older than DocId 20
        let turns = vec![
            (DocId::new(10), emb.clone()),
            (DocId::new(20), emb.clone()),
        ];

        let pairs = detect_near_duplicates(&turns, 0.95);
        assert_eq!(pairs.len(), 1);
        let (older, newer) = pairs[0];
        assert_eq!(older, DocId::new(10), "The older turn (smaller DocId) must be flagged for tombstoning");
        assert_eq!(newer, DocId::new(20));
    }

    #[test]
    fn test_detect_near_duplicates_sub_threshold() {
        let emb_a = vec![1.0, 0.0, 0.0, 0.0];
        let emb_b = vec![0.0, 1.0, 0.0, 0.0]; // Cosine sim = 0.0 < 0.95
        let turns = vec![
            (DocId::new(10), emb_a),
            (DocId::new(20), emb_b),
        ];

        let pairs = detect_near_duplicates(&turns, 0.95);
        assert!(pairs.is_empty(), "Embeddings below similarity threshold must not trigger near-duplicate detection");
    }

    #[test]
    fn test_empty_input_no_panic() {
        let config = NremConfig::default();
        let res = run_nrem_phase(&[], &config);
        assert_eq!(res.segments_created, 0);
        assert!(res.duplicates_tombstoned.is_empty());
        assert!(res.cascade_edge_tombstones_needed.is_empty());
    }

    #[test]
    fn test_min_turns_per_segment_merging() {
        // Seg 1: 5 turns (Cluster A)
        // Seg 2: 1 turn (Cluster B - under min_turns_per_segment = 3)
        let mut turns = Vec::new();
        let emb_a = vec![1.0, 0.0, 0.0, 0.0];
        for i in 1..=5 {
            turns.push((DocId::new(i), emb_a.clone()));
        }
        let emb_b = vec![0.0, 1.0, 0.0, 0.0];
        turns.push((DocId::new(6), emb_b));

        let config = NremConfig {
            min_turns_per_segment: 3,
            max_turns_per_segment: 20,
            near_duplicate_cosine_threshold: 0.95,
        };

        let segments = group_turns_into_segments(&turns, &config);
        assert_eq!(
            segments.len(),
            1,
            "Segment under min_turns_per_segment must be merged into neighboring segment"
        );
        assert_eq!(segments[0].turn_ids.len(), 6);
    }
}
