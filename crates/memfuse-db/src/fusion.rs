//! Reciprocal Rank Fusion implementation.

// FILE-CONTEXT
// STAND: 2026-08-29T05:41:20Z (SESSION: f7999509)
// ZWECK: Reciprocal Rank Fusion (RRF) — vereint HNSW, BM25 und Graph-Ränge
// INVARIANTEN: k=60 Standard. Signale werden als Ränge fusioniert (NICHT rohe Scores).
//              Keine Score-Normalisierung nötig (Hauptvorteil von RRF, ADR-003).
// NICHT-OFFENSICHTLICH: Es existieren ZWEI öffentliche Funktionen:
//   1. `reciprocal_rank_fusion()` — gleichgewichtet (1.0 pro Signal)
//   2. `weighted_reciprocal_rank_fusion()` — mit Name + Gewicht pro Signal
//   NIEMALS eine dritte `execute_rrf()`-Funktion anlegen — sie würde diese duplizieren.
// SIEHE AUCH: DECISIONS.md ADR-003, crates/memfuse-db/AGENTS.md §4-Signal Fusion

use crate::{ProvenanceRecord, SearchResult};
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};

/// Konfiguration für den Resonanz-Kohärenz-Bonus (F-09).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResonanceConfig {
    /// Exponent β für den Kohärenz-Bonus. Default: 0.5.
    pub beta: f32,
    /// Boost-Stärke γ. Default: 0.3.
    pub gamma: f32,
}

impl Default for ResonanceConfig {
    fn default() -> Self {
        Self {
            beta: 0.5,
            gamma: 0.3,
        }
    }
}

/// Wendet Resonanz-Kohärenz-Bonus auf fusionierte Ergebnisse an.
///
/// INVARIANTE INV-PROV-2: `signal_contributions` bleiben unverändert (unboosted).
/// `coherence_bonus` wird in `provenance.coherence_bonus` geschrieben.
///
/// Feature-Flag: Nur aufrufen wenn `coherence-bonus-fusion` aktiv.
#[cfg(feature = "coherence-bonus-fusion")]
pub fn apply_resonance_bonus(
    results: Vec<SearchResult>,
    valid_signal_count: usize,
    config: &ResonanceConfig,
) -> Vec<SearchResult> {
    if valid_signal_count == 0 {
        return results;
    }
    let beta = config.beta.clamp(0.1, 2.0);
    let gamma = config.gamma.clamp(0.0, 1.0);
    let mut results: Vec<_> = results
        .into_iter()
        .map(|mut r| {
            if !r.score.is_finite() {
                tracing::error!(
                    doc_id = %r.id,
                    raw_score = r.score,
                    "apply_resonance_bonus: non-finite score entering resonance bonus stage"
                );
            }
            let signal_count = r.matched_signals.len();
            let coherence = (signal_count as f32 / valid_signal_count as f32).powf(beta);
            let bonus = gamma * coherence;
            r.score *= 1.0 + bonus;
            if let Some(ref mut prov) = r.provenance {
                prov.coherence_bonus = bonus;
            }
            r
        })
        .collect();

    results.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    // NC-6: NaN/Inf-Scores ans Ende (stable partition erhält Reihenfolge unter ihnen)
    results.sort_by(|a, b| {
        match (a.score.is_finite(), b.score.is_finite()) {
            (true, false) => std::cmp::Ordering::Less,    // finite vor non-finite
            (false, true) => std::cmp::Ordering::Greater, // non-finite nach finite
            _             => b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)),
        }
    });

    results
}

struct HeapEntry {
    result: SearchResult,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for HeapEntry {}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // We want BinaryHeap (a max-heap by default) to keep the worst item at the top (peek),
        // so that peek() returns the candidate with the lowest score (or highest ID on tie).
        // Therefore, lower score => Greater priority in max-heap.
        other
            .result
            .score
            .total_cmp(&self.result.score)
            .then_with(|| self.result.id.cmp(&other.result.id))
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Identifies the kind of search signal used during fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalKind {
    /// Vector (semantic k-NN) search signal.
    Vector,
    /// Text (BM25 keyword) search signal.
    Text,
    /// Graph (traversal / PageRank) search signal.
    Graph,
    #[cfg(feature = "edge-reinforcement-learning")]
    /// Edge-reinforcement weight signal (F-03).
    EdgeReinforcement,
}

impl SignalKind {
    /// Identifies `SignalKind` from a signal name string (e.g. "vector", "text", "graph", "edge-reinforcement").
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "vector" | "vec" => Some(SignalKind::Vector),
            "text" | "bm25" | "keyword" => Some(SignalKind::Text),
            "graph" => Some(SignalKind::Graph),
            #[cfg(feature = "edge-reinforcement-learning")]
            "edge-reinforcement"
            | "cooccurrence"
            | "traversal-reinforcement"
            | "synaptic"
            | "hebbian" => Some(SignalKind::EdgeReinforcement),
            _ => None,
        }
    }
}

/// Configures signal priority order for metadata merging during Reciprocal Rank Fusion.
///
/// The metadata merge strategy uses a "First-Wins" policy: for any given key, the value from
/// the earliest processed signal set is kept. `MetadataMergePriority` controls the order in which
/// signal sets are processed during metadata merging.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MetadataMergePriority {
    /// Vector metadata is processed first (default behavior). Order: Vector, Text, Graph.
    #[default]
    VectorFirst,
    /// Text metadata is processed first. Order: Text, Vector, Graph.
    TextFirst,
    /// Graph metadata is processed first (preserving graph entity/community metadata). Order: Graph, Vector, Text.
    GraphFirst,
    /// Custom signal priority order. Signals listed earlier have precedence over signals listed later.
    Custom(Vec<SignalKind>),
}

impl MetadataMergePriority {
    /// Returns the precedence rank (lower number = processed earlier) for a given signal name.
    pub fn signal_rank(&self, signal_name: &str) -> usize {
        let kind = SignalKind::from_name(signal_name);
        let order = match self {
            MetadataMergePriority::VectorFirst => {
                vec![SignalKind::Vector, SignalKind::Text, SignalKind::Graph]
            }
            MetadataMergePriority::TextFirst => {
                vec![SignalKind::Text, SignalKind::Vector, SignalKind::Graph]
            }
            MetadataMergePriority::GraphFirst => {
                vec![SignalKind::Graph, SignalKind::Vector, SignalKind::Text]
            }
            MetadataMergePriority::Custom(custom_order) => custom_order.clone(),
        };

        if let Some(k) = kind {
            if let Some(pos) = order.iter().position(|&x| x == k) {
                return pos;
            }
        }
        usize::MAX
    }
}

/// Baut einen ProvenanceRecord aus den verfügbaren Signal-Scores und optionalen Signal-Gewichten.
/// Erfüllt INV-PROV-1: sum(contributions.rrf_contribution) ≈ unboosted RRF score (|Δ| < 1e-6).
/// Die Invariante wird in Debug-Builds per `debug_assert!` überprüft, wenn `expected_total` (Ground-Truth RRF score) angegeben ist.
#[allow(clippy::too_many_arguments)]
pub fn build_provenance(
    vector_distance: Option<f32>,
    vector_rank: Option<u32>,
    vector_weight: Option<f32>,
    bm25_score: Option<f32>,
    bm25_rank: Option<u32>,
    text_weight: Option<f32>,
    graph_score: Option<f32>,
    graph_rank: Option<u32>,
    graph_weight: Option<f32>,
    rerank_score: Option<f32>,
    rrf_k: f32,
    source_collection: Option<String>,
    index_type: Option<String>,
    expected_total: Option<f32>,
) -> ProvenanceRecord {
    let mut signal_ranks = HashMap::new();
    let mut signal_contributions = HashMap::new();

    let v_w = vector_weight.unwrap_or(1.0);
    let t_w = text_weight.unwrap_or(1.0);
    let g_w = graph_weight.unwrap_or(1.0);

    if let (Some(score), Some(rank)) = (vector_distance, vector_rank) {
        signal_ranks.insert("vector".to_string(), rank);
        let rrf_contrib = v_w / (rrf_k + rank as f32);
        signal_contributions.insert(
            "vector".to_string(),
            crate::SignalContribution {
                raw_score: score,
                rank,
                rrf_contribution: rrf_contrib,
            },
        );
    }

    if let (Some(score), Some(rank)) = (bm25_score, bm25_rank) {
        signal_ranks.insert("text".to_string(), rank);
        let rrf_contrib = t_w / (rrf_k + rank as f32);
        signal_contributions.insert(
            "text".to_string(),
            crate::SignalContribution {
                raw_score: score,
                rank,
                rrf_contribution: rrf_contrib,
            },
        );
    }

    if let (Some(score), Some(rank)) = (graph_score, graph_rank) {
        signal_ranks.insert("graph".to_string(), rank);
        let rrf_contrib = g_w / (rrf_k + rank as f32);
        signal_contributions.insert(
            "graph".to_string(),
            crate::SignalContribution {
                raw_score: score,
                rank,
                rrf_contribution: rrf_contrib,
            },
        );
    }

    let record = ProvenanceRecord {
        vector_distance,
        bm25_score,
        graph_score,
        rerank_score,
        signal_ranks,
        source_collection,
        index_type,
        signal_contributions,
        coherence_bonus: 0.0,
    };

    if let Some(expected) = expected_total {
        let expected_rrf: f32 = record
            .signal_contributions
            .values()
            .map(|c| c.rrf_contribution)
            .sum();
        if !(expected_rrf - expected).abs().lt(&1e-6) {
            tracing::error!(
                expected_rrf,
                expected,
                "INV-PROV-1 violation in build_provenance: sum of contributions ({expected_rrf}) != expected RRF score ({expected})"
            );
        }
    }

    record
}

/// Fuses multiple sets of ranked search results into a single ranked list using Reciprocal Rank Fusion (RRF).
/// RRF score = sum(1 / (k + rank)) for each result set, where k = 60 by default.
pub fn reciprocal_rank_fusion(
    result_sets: Vec<Vec<SearchResult>>,
    max_results: usize,
) -> Vec<SearchResult> {
    let weighted_sets = result_sets
        .into_iter()
        .map(|set| ("unnamed".to_string(), set, 1.0))
        .collect();
    weighted_reciprocal_rank_fusion(weighted_sets, max_results)
}

/// Merge-Semantik:
/// - JSON-Objects: rekursives Merging, First-Wins bei echter Kollision.
/// - Scalare Kollisionen (target ≠ source, beide nicht-Object): BEWUSSTE Array-Konvertierung
///   `[target_value, source_value]`. Kein First-Wins — verhindert stillen Informationsverlust
///   wenn mehrere Fusion-Signale denselben Key mit verschiedenen Werten liefern.
///
/// ⚠️ KONTRAKT FÜR CONSUMER: Code der `metadata[key].as_f64()` (o.ä.) für Felder
/// aus mehreren Fusion-Signalen aufruft MUSS damit rechnen, dass der Wert ein
/// `serde_json::Value::Array` statt eines Scalars ist.
fn merge_metadata(target: &mut Option<serde_json::Value>, source: Option<serde_json::Value>) {
    match (target, source) {
        (Some(t_val), Some(s_val)) => {
            if let (Some(t_obj), Some(s_obj)) = (t_val.as_object_mut(), s_val.as_object()) {
                for (k, v) in s_obj {
                    if !t_obj.contains_key(k) {
                        t_obj.insert(k.clone(), v.clone());
                    }
                }
            } else {
                // Scalar-Kollision: Array-Konvertierung ist bewusste Entscheidung — siehe Doc-Kommentar.
                if t_val != &s_val {
                    let arr = vec![t_val.clone(), s_val.clone()];
                    *t_val = serde_json::Value::Array(arr);
                }
            }
        }
        (t @ None, Some(s_val)) => {
            *t = Some(s_val);
        }
        _ => {}
    }
}

/// Weighted Reciprocal Rank Fusion with default signal metadata priority (`VectorFirst`).
/// Multiplies the RRF contribution of each search signal set by its configured weight.
///
/// Accepts tuples of `(signal_name, result_set, weight)`.
pub fn weighted_reciprocal_rank_fusion(
    result_sets: Vec<(String, Vec<SearchResult>, f32)>,
    max_results: usize,
) -> Vec<SearchResult> {
    weighted_reciprocal_rank_fusion_with_options(
        result_sets,
        max_results,
        MetadataMergePriority::default(),
        true,
        None,
    )
}

/// Weighted Reciprocal Rank Fusion with explicit metadata merge priority.
///
/// Multiplies the RRF contribution of each search signal set by its configured weight,
/// and applies metadata merging in the order dictated by `priority`.
pub fn weighted_reciprocal_rank_fusion_with_priority(
    result_sets: Vec<(String, Vec<SearchResult>, f32)>,
    max_results: usize,
    priority: MetadataMergePriority,
) -> Vec<SearchResult> {
    weighted_reciprocal_rank_fusion_with_options(result_sets, max_results, priority, true, None)
}

/// Weighted Reciprocal Rank Fusion with explicit metadata merge priority and provenance toggle.
pub fn weighted_reciprocal_rank_fusion_with_options(
    mut result_sets: Vec<(String, Vec<SearchResult>, f32)>,
    max_results: usize,
    priority: MetadataMergePriority,
    include_provenance: bool,
    resonance_config: Option<&ResonanceConfig>,
) -> Vec<SearchResult> {
    if max_results == 0 {
        return Vec::new();
    }

    let mut valid_signal_count = 0usize;
    let _ = &resonance_config;

    // Sort result sets according to configured metadata merge priority.
    // Stable sort preserves original relative order for signals with equal rank.
    result_sets.sort_by_key(|(signal_name, _, _)| priority.signal_rank(signal_name));

    // The constant k=60 is the industry standard (Cormack et al., 2009).
    // It balances the precision/recall trade-off by smoothing rank impact:
    // higher k prevents top-ranked outliers in one signal from completely dominating,
    // while ensuring items appearing in multiple search signals accumulate significant boost.
    let k = 60;
    // Map: id -> (score, metadata, matched_signals, provenance)
    let mut fused: HashMap<
        String,
        (
            f32,
            Option<serde_json::Value>,
            Vec<String>,
            ProvenanceRecord,
        ),
    > = HashMap::new();

    for (signal_name, result_set, weight) in result_sets {
        if !weight.is_finite() || weight <= 0.0 {
            tracing::warn!(
                signal = %signal_name,
                weight,
                "RRF fusion: non-finite or non-positive weight skipped"
            );
            continue;
        }
        valid_signal_count += 1;
        let signal_kind = SignalKind::from_name(&signal_name);
        for (rank, doc) in result_set.into_iter().enumerate() {
            if !doc.score.is_finite() {
                tracing::error!(
                    signal = %signal_name,
                    doc_id = %doc.id,
                    raw_score = doc.score,
                    "RRF fusion: non-finite raw score from upstream signal detected"
                );
            }
            let score = weight / (k as f32 + rank as f32 + 1.0);
            debug_assert!(
                score.is_finite(),
                "RRF score must be finite after weight validation"
            );
            let entry = fused
                .entry(doc.id)
                .or_insert_with(|| (0.0, None, Vec::new(), ProvenanceRecord::default()));
            entry.0 += score;
            merge_metadata(&mut entry.1, doc.metadata);
            if !signal_name.is_empty()
                && signal_name != "unnamed"
                && !entry.2.contains(&signal_name)
            {
                entry.2.push(signal_name.clone());
            }

            if !signal_name.is_empty() && signal_name != "unnamed" {
                entry
                    .3
                    .signal_ranks
                    .insert(signal_name.clone(), (rank + 1) as u32);

                // Record per-signal RRF contribution (INV-PROV-1)
                entry.3.signal_contributions.insert(
                    signal_name.clone(),
                    crate::SignalContribution {
                        raw_score: doc.score,
                        rank: (rank + 1) as u32,
                        rrf_contribution: score,
                    },
                );
            }

            match signal_kind {
                Some(SignalKind::Vector) => {
                    if entry.3.vector_distance.is_none() {
                        entry.3.vector_distance = Some(doc.score);
                    }
                    if entry.3.index_type.is_none() {
                        entry.3.index_type = Some("hnsw".to_string());
                    }
                }
                Some(SignalKind::Text) => {
                    if entry.3.bm25_score.is_none() {
                        entry.3.bm25_score = Some(doc.score);
                    }
                    if entry.3.index_type.is_none() {
                        entry.3.index_type = Some("bm25".to_string());
                    }
                }
                Some(SignalKind::Graph) => {
                    if entry.3.graph_score.is_none() {
                        entry.3.graph_score = Some(doc.score);
                    }
                    if entry.3.index_type.is_none() {
                        entry.3.index_type = Some("graph".to_string());
                    }
                }
                #[cfg(feature = "edge-reinforcement-learning")]
                Some(SignalKind::EdgeReinforcement) => {
                    if entry.3.graph_score.is_none() {
                        entry.3.graph_score = Some(doc.score);
                    }
                    if entry.3.index_type.is_none() {
                        entry.3.index_type = Some("edge-reinforcement".to_string());
                    }
                }
                None => {}
            }

            if let Some(doc_prov) = doc.provenance {
                if entry.3.vector_distance.is_none() {
                    entry.3.vector_distance = doc_prov.vector_distance;
                }
                if entry.3.bm25_score.is_none() {
                    entry.3.bm25_score = doc_prov.bm25_score;
                }
                if entry.3.graph_score.is_none() {
                    entry.3.graph_score = doc_prov.graph_score;
                }
                if entry.3.rerank_score.is_none() {
                    entry.3.rerank_score = doc_prov.rerank_score;
                }
                if entry.3.source_collection.is_none() {
                    entry.3.source_collection = doc_prov.source_collection;
                }
                if entry.3.index_type.is_none() {
                    entry.3.index_type = doc_prov.index_type;
                }
                for (sig, r) in doc_prov.signal_ranks {
                    entry.3.signal_ranks.entry(sig).or_insert(r);
                }
                for (sig, contrib) in doc_prov.signal_contributions {
                    entry.3.signal_contributions.entry(sig).or_insert(contrib);
                }
            }
        }
    }

    // AGT-DB-001 [CONCURRENCY][MAJOR]: Deterministic tie-breaking via secondary sort by ID.
    // Bounded Min-Heap O(U log K) top-K selection instead of full O(U log U) sort.
    // Bound capacity to min(fused.len(), max_results) to avoid allocation overflow when max_results is large (e.g. usize::MAX).
    let target_cap = fused.len().min(max_results);
    let mut heap = BinaryHeap::with_capacity(target_cap.saturating_add(1));

    for (id, (score, metadata, matched_signals, prov)) in fused {
        let provenance = if include_provenance
            && (prov.vector_distance.is_some()
                || prov.bm25_score.is_some()
                || prov.graph_score.is_some()
                || prov.rerank_score.is_some()
                || !prov.signal_ranks.is_empty()
                || prov.source_collection.is_some()
                || prov.index_type.is_some())
        {
            let final_prov = if prov.signal_contributions.is_empty() {
                build_provenance(
                    prov.vector_distance,
                    prov.signal_ranks.get("vector").copied(),
                    None,
                    prov.bm25_score,
                    prov.signal_ranks
                        .get("text")
                        .copied()
                        .or_else(|| prov.signal_ranks.get("bm25").copied()),
                    None,
                    prov.graph_score,
                    prov.signal_ranks.get("graph").copied(),
                    None,
                    prov.rerank_score,
                    k as f32,
                    prov.source_collection,
                    prov.index_type,
                    None,
                )
            } else {
                prov
            };

            // INV-PROV-1: Sum of per-signal RRF contributions must equal entry score
            if !final_prov.signal_contributions.is_empty() {
                let sum_contrib: f32 = final_prov
                    .signal_contributions
                    .values()
                    .map(|c| c.rrf_contribution)
                    .sum();
                debug_assert!(
                    (sum_contrib - score).abs() < 1e-6,
                    "INV-PROV-1: sum_contrib={sum_contrib} ≠ score={score} for doc_id={id} — \
                     provenance data inconsistent"
                );
                if !(sum_contrib - score).abs().lt(&1e-6) {
                    tracing::error!(
                        doc_id = %id,
                        sum_contrib,
                        score,
                        provenance_consistent = false,
                        "INV-PROV-1 violation: sum of signal contributions does not match entry score"
                    );
                }
            }

            Some(final_prov)
        } else {
            None
        };

        let entry = HeapEntry {
            result: SearchResult {
                id,
                score,
                metadata,
                matched_signals,
                provenance,
            },
        };

        if heap.len() < max_results {
            heap.push(entry);
        } else if let Some(worst) = heap.peek() {
            if entry < *worst {
                heap.pop();
                heap.push(entry);
            }
        }
    }

    let results: Vec<SearchResult> = heap
        .into_sorted_vec()
        .into_iter()
        .map(|e| e.result)
        .collect();

    #[cfg(feature = "coherence-bonus-fusion")]
    let results = if let Some(cfg) = resonance_config {
        apply_resonance_bonus(results, valid_signal_count, cfg)
    } else {
        results
    };

    #[cfg(not(feature = "coherence-bonus-fusion"))]
    let _ = valid_signal_count;

    results
}

/// Converts optional FusionWeights into (vector, text, graph) weight tuple.
pub fn weights_to_signal_factors(weights: Option<&memfuse_core::FusionWeights>) -> (f32, f32, f32) {
    match weights {
        Some(w) => (w.vector(), w.text(), w.graph()),
        None => (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_dual_signal_higher_than_single_signal() {
        let set1 = vec![SearchResult {
            id: "doc_both".to_string(),
            score: 0.99,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        }];
        let set2 = vec![
            SearchResult {
                id: "doc_both".to_string(),
                score: 0.95,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "doc_single".to_string(),
                score: 0.99,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        ];

        let fused = reciprocal_rank_fusion(vec![set1, set2], 10);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].id, "doc_both", "Document ranked #1 in both signals must score higher than doc ranked #1 in only one signal");
        assert_eq!(fused[1].id, "doc_single");
        assert!(fused[0].score > fused[1].score);
    }

    #[test]
    fn test_rrf_combines_result_sets() {
        let vectors = vec![
            SearchResult {
                id: "doc_a".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "doc_b".to_string(),
                score: 0.8,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "doc_c".to_string(),
                score: 0.7,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        ];

        let keywords = vec![
            SearchResult {
                id: "doc_b".to_string(),
                score: 2.1,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "doc_d".to_string(),
                score: 1.5,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        ];

        let fused = reciprocal_rank_fusion(vec![vectors, keywords], 5);

        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["doc_b", "doc_a", "doc_d", "doc_c"]);
    }

    #[test]
    fn fusion_empty_inputs_returns_empty() {
        let result = reciprocal_rank_fusion(vec![vec![], vec![]], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn fusion_respects_max_results() {
        let large_set: Vec<SearchResult> = (0..100)
            .map(|i| SearchResult {
                id: format!("doc-{i}"),
                score: i as f32 / 100.0,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            })
            .collect();
        let result = reciprocal_rank_fusion(vec![large_set], 5);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn fusion_ignores_zero_or_negative_weight() {
        let set1 = vec![SearchResult {
            id: "doc-1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        }];
        let set2 = vec![SearchResult {
            id: "doc-2".to_string(),
            score: 0.8,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        }];

        let result = weighted_reciprocal_rank_fusion(
            vec![
                ("signal1".to_string(), set1, 1.0),
                ("signal2".to_string(), set2, 0.0),
            ],
            10,
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "doc-1");
    }

    #[test]
    fn test_rrf_empty_inputs_return_empty() {
        let fused = reciprocal_rank_fusion(vec![], 5);
        assert!(
            fused.is_empty(),
            "Empty input sets should return empty results"
        );

        let fused2 = reciprocal_rank_fusion(vec![vec![], vec![]], 5);
        assert!(
            fused2.is_empty(),
            "Inputs with empty inner sets should return empty results"
        );
    }

    #[test]
    fn test_rrf_truncates_max_results() {
        let vectors: Vec<SearchResult> = (0..10)
            .map(|i| SearchResult {
                id: format!("doc_{}", i),
                score: 0.99,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            })
            .collect();

        let keywords: Vec<SearchResult> = (5..15)
            .map(|i| SearchResult {
                id: format!("doc_{}", i),
                score: 0.88,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            })
            .collect();

        // Pass 10 + 10 elements. The limit is exclusively 3.
        let fused = reciprocal_rank_fusion(vec![vectors, keywords], 3);
        assert_eq!(
            fused.len(),
            3,
            "Result must be strictly truncated to max_results"
        );
    }

    #[test]
    fn test_rrf_identical_ranks() {
        let vectors = vec![
            SearchResult {
                id: "Y".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "X".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        ];
        let keywords = vec![
            SearchResult {
                id: "X".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "Y".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        ];

        // AGT-DB-001: Repeat 20 times to prove output ordering is strictly deterministic across iterations
        for _ in 0..20 {
            let fused = reciprocal_rank_fusion(vec![vectors.clone(), keywords.clone()], 2);
            assert_eq!(fused.len(), 2);
            assert_eq!(
                fused[0].id, "X",
                "Secondary sort by ID must place X before Y"
            );
            assert_eq!(fused[1].id, "Y");
            assert!((fused[0].score - fused[1].score).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn test_rrf_metadata_merging_and_matched_signals() {
        let vec_set = (
            "vector".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.9,
                metadata: Some(serde_json::json!({"vec_key": "val1", "shared_key": "from_vector"})),
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );
        let graph_set = (
            "graph".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.8,
                metadata: Some(
                    serde_json::json!({"graph_key": "val2", "shared_key": "from_graph"}),
                ),
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );

        let fused = weighted_reciprocal_rank_fusion(vec![vec_set, graph_set], 1);
        assert_eq!(fused.len(), 1);
        let doc = &fused[0];
        assert_eq!(doc.id, "doc1");

        // Verify metadata merging (earlier signal key is retained, missing keys supplemented)
        if let Some(serde_json::Value::Object(meta)) = &doc.metadata {
            assert_eq!(meta.get("vec_key"), Some(&serde_json::json!("val1")));
            assert_eq!(meta.get("graph_key"), Some(&serde_json::json!("val2")));
            assert_eq!(
                meta.get("shared_key"),
                Some(&serde_json::json!("from_vector"))
            );
        } else {
            panic!("Expected metadata object");
        }

        // Verify matched signals tracking
        assert_eq!(doc.matched_signals, vec!["vector", "graph"]);
    }

    #[test]
    fn test_merge_metadata_scalar_collision_produces_array() {
        use serde_json::json;

        let mut scalar_target = Some(json!(0.9));
        let scalar_source = Some(json!(0.7));
        merge_metadata(&mut scalar_target, scalar_source);

        // INTENTIONAL: Scalar-Kollision -> Array (nicht First-Wins). Verifiziert bewusste Designentscheidung.
        assert_eq!(scalar_target, Some(json!([0.9, 0.7])));
    }

    #[test]
    fn test_metadata_merge_priority_colliding_keys() {
        let vec_set = (
            "vector".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.9,
                metadata: Some(serde_json::json!({
                    "shared_key": "from_vector",
                    "vec_only": "vec_val"
                })),
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );
        let text_set = (
            "text".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.85,
                metadata: Some(serde_json::json!({
                    "shared_key": "from_text",
                    "text_only": "text_val"
                })),
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );
        let graph_set = (
            "graph".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.8,
                metadata: Some(serde_json::json!({
                    "shared_key": "from_graph",
                    "graph_only": "graph_val"
                })),
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );

        // 1. VectorFirst (Default) -> Vector wins shared_key
        let fused_vec = weighted_reciprocal_rank_fusion_with_priority(
            vec![vec_set.clone(), text_set.clone(), graph_set.clone()],
            1,
            MetadataMergePriority::VectorFirst,
        );
        if let Some(serde_json::Value::Object(meta_vec)) = &fused_vec[0].metadata {
            assert_eq!(
                meta_vec.get("shared_key"),
                Some(&serde_json::json!("from_vector"))
            );
            assert_eq!(
                meta_vec.get("vec_only"),
                Some(&serde_json::json!("vec_val"))
            );
            assert_eq!(
                meta_vec.get("text_only"),
                Some(&serde_json::json!("text_val"))
            );
            assert_eq!(
                meta_vec.get("graph_only"),
                Some(&serde_json::json!("graph_val"))
            );
        } else {
            panic!("Expected metadata object");
        }

        // 2. TextFirst -> Text wins shared_key
        let fused_text = weighted_reciprocal_rank_fusion_with_priority(
            vec![vec_set.clone(), text_set.clone(), graph_set.clone()],
            1,
            MetadataMergePriority::TextFirst,
        );
        if let Some(serde_json::Value::Object(meta_text)) = &fused_text[0].metadata {
            assert_eq!(
                meta_text.get("shared_key"),
                Some(&serde_json::json!("from_text"))
            );
        } else {
            panic!("Expected metadata object");
        }

        // 3. GraphFirst -> Graph wins shared_key
        let fused_graph = weighted_reciprocal_rank_fusion_with_priority(
            vec![vec_set.clone(), text_set.clone(), graph_set.clone()],
            1,
            MetadataMergePriority::GraphFirst,
        );
        if let Some(serde_json::Value::Object(meta_graph)) = &fused_graph[0].metadata {
            assert_eq!(
                meta_graph.get("shared_key"),
                Some(&serde_json::json!("from_graph"))
            );
        } else {
            panic!("Expected metadata object");
        }

        // 4. Custom priority (Graph -> Text -> Vector) -> Graph wins shared_key
        let fused_custom = weighted_reciprocal_rank_fusion_with_priority(
            vec![vec_set.clone(), text_set.clone(), graph_set.clone()],
            1,
            MetadataMergePriority::Custom(vec![
                SignalKind::Graph,
                SignalKind::Text,
                SignalKind::Vector,
            ]),
        );
        if let Some(serde_json::Value::Object(meta_custom)) = &fused_custom[0].metadata {
            assert_eq!(
                meta_custom.get("shared_key"),
                Some(&serde_json::json!("from_graph"))
            );
        } else {
            panic!("Expected metadata object");
        }
    }

    #[test]
    fn test_weights_to_signal_factors_none_returns_equal_thirds() {
        let (vec_w, text_w, graph_w) = weights_to_signal_factors(None);
        // Anti-mirroring check: Expected 1/3 = 0.33333334
        assert!((vec_w - 0.33333334).abs() < 1e-5);
        assert!((text_w - 0.33333334).abs() < 1e-5);
        assert!((graph_w - 0.33333334).abs() < 1e-5);
    }

    #[test]
    fn test_weights_to_signal_factors_some_returns_exact_weights() {
        use memfuse_core::FusionWeights;
        if let Ok(weights) = FusionWeights::new(0.5, 0.3, 0.2) {
            let (v, t, g) = weights_to_signal_factors(Some(&weights));
            assert!((v - 0.5).abs() < 1e-5);
            assert!((t - 0.3).abs() < 1e-5);
            assert!((g - 0.2).abs() < 1e-5);
        } else {
            panic!("Expected valid weights");
        }
    }

    #[test]
    fn test_bounded_min_heap_top_k_selection() {
        let set = (0..50)
            .map(|i| SearchResult {
                id: format!("doc_{:02}", i),
                score: 0.0,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            })
            .collect();
        // In RRF, rank 0 (doc_00) gets score 1/(60+1) = 0.01639..., rank 49 gets score 1/(60+50) = 0.00909...
        // Top 5 results must be doc_00, doc_01, doc_02, doc_03, doc_04 in exact order.
        let fused = weighted_reciprocal_rank_fusion(vec![("vector".to_string(), set, 1.0)], 5);
        assert_eq!(fused.len(), 5);
        assert_eq!(fused[0].id, "doc_00");
        assert_eq!(fused[1].id, "doc_01");
        assert_eq!(fused[2].id, "doc_02");
        assert_eq!(fused[3].id, "doc_03");
        assert_eq!(fused[4].id, "doc_04");
    }

    #[test]
    fn test_weighted_rrf_zero_max_results_returns_empty() {
        let set = vec![SearchResult {
            id: "doc1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        }];
        let fused = weighted_reciprocal_rank_fusion(vec![("vec".to_string(), set, 1.0)], 0);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_weighted_rrf_negative_weights_ignored() {
        let set = vec![SearchResult {
            id: "doc1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        }];
        let fused = weighted_reciprocal_rank_fusion(vec![("vec".to_string(), set, -0.5)], 10);
        assert!(fused.is_empty());
    }

    #[test]
    fn test_build_provenance_invariant_consistent() {
        let rank = 1u32;
        let weight = 1.0f32;
        let k = 60.0f32;
        let expected_contrib = weight / (k + rank as f32);

        let prov = build_provenance(
            Some(0.95),
            Some(rank),
            Some(weight),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            k,
            Some("test_collection".to_string()),
            Some("hnsw".to_string()),
            Some(expected_contrib),
        );

        let sum: f32 = prov
            .signal_contributions
            .values()
            .map(|c| c.rrf_contribution)
            .sum();
        assert!((sum - expected_contrib).abs() < 1e-6);
    }

    #[test]
    fn test_build_provenance_invariant_inconsistent_logs_error() {
        let rank = 1u32;
        let weight = 1.0f32;
        let k = 60.0f32;
        let wrong_expected = 0.999f32; // Discrepancy > 1e-6

        let prov = build_provenance(
            Some(0.95),
            Some(rank),
            Some(weight),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            k,
            Some("test_collection".to_string()),
            Some("hnsw".to_string()),
            Some(wrong_expected),
        );
        assert!(!prov.signal_contributions.is_empty());
    }

    #[test]
    fn test_heap_entry_nan_score_sorts_to_worst_position() {
        use std::collections::BinaryHeap;

        let mut heap = BinaryHeap::new();
        heap.push(HeapEntry {
            result: SearchResult {
                id: "doc1".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        });
        heap.push(HeapEntry {
            result: SearchResult {
                id: "doc2".to_string(),
                score: 0.5,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        });
        heap.push(HeapEntry {
            result: SearchResult {
                id: "doc_nan".to_string(),
                score: f32::NAN,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        });

        // Extract all entries and verify NaN entry is last (worst)
        let mut extracted = vec![];
        while let Some(entry) = heap.pop() {
            extracted.push(entry.result.id.clone());
        }

        assert_eq!(
            extracted.last().map(|s| s.as_str()),
            Some("doc_nan"),
            "NaN score entry must be at the end (worst position) after total_cmp\nExtracted order: {:?}",
            extracted
        );
    }

    #[test]
    fn test_inv_prov1_violation_logged_in_release_mode() {
        // Construct a result with inconsistent signal_contributions
        let score = 0.5;
        let mut prov = ProvenanceRecord::default();
        prov.signal_contributions.insert(
            "vector".to_string(),
            crate::SignalContribution {
                raw_score: 0.9,
                rank: 1,
                rrf_contribution: 0.1, // Intentionally wrong sum (0.1 != 0.5)
            },
        );

        // Call the fusion function with a result that violates INV-PROV-1
        let result = SearchResult {
            id: "test_doc".to_string(),
            score,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: Some(prov),
        };

        // The test verifies that the function doesn't panic (no fatal error)
        // In a real scenario with tracing-test, we could capture the error log;
        // for now, we just ensure no panic occurs
        let results = weighted_reciprocal_rank_fusion_with_options(
            vec![("vector".to_string(), vec![result], 1.0)],
            10,
            MetadataMergePriority::default(),
            true,
            None,
        );

        // Function should complete without panic despite invariant violation
        assert_eq!(
            results.len(),
            1,
            "Fusion should complete and return results despite INV-PROV-1 violation"
        );
    }

    #[test]
    #[cfg(feature = "coherence-bonus-fusion")]
    fn test_resonance_bonus_multi_signal_beats_single_signal() {
        let doc_multi = |score: f32| SearchResult {
            id: "doc_multi".to_string(),
            score,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        };

        let make_set = |name: &str, include_single: bool| {
            let mut list = Vec::new();
            if include_single {
                list.push(SearchResult {
                    id: "doc_single".to_string(),
                    score: 0.99,
                    metadata: None,
                    matched_signals: vec![],
                    provenance: None,
                });
            }
            for i in 0..124 {
                list.push(SearchResult {
                    id: format!("filler_{name}_{i}"),
                    score: 0.5,
                    metadata: None,
                    matched_signals: vec![],
                    provenance: None,
                });
            }
            list.push(doc_multi(0.8));
            (name.to_string(), list, 1.0)
        };

        let s1 = make_set("vector", true);
        let s2 = make_set("text", false);
        let s3 = make_set("graph", false);

        let cfg = ResonanceConfig::default();
        let fused = weighted_reciprocal_rank_fusion_with_options(
            vec![s1, s2, s3],
            10,
            MetadataMergePriority::default(),
            true,
            Some(&cfg),
        );

        assert_eq!(
            fused[0].id, "doc_multi",
            "Multi-signal doc with resonance bonus must beat single-signal doc despite lower unboosted score"
        );
        assert!(fused[0].score > fused[1].score);
    }

    #[test]
    #[cfg(feature = "coherence-bonus-fusion")]
    fn test_resonance_bonus_provenance_coherence_set() {
        let set1 = (
            "vector".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );
        let set2 = (
            "text".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.8,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );

        let cfg = ResonanceConfig::default();
        let fused = weighted_reciprocal_rank_fusion_with_options(
            vec![set1, set2],
            10,
            MetadataMergePriority::default(),
            true,
            Some(&cfg),
        );

        assert_eq!(fused.len(), 1);
        let prov = match fused[0].provenance.as_ref() {
            Some(p) => p,
            None => panic!("provenance present"),
        };
        assert!(
            prov.coherence_bonus > 0.0,
            "coherence_bonus must be > 0.0 for multi-signal document"
        );
    }

    #[test]
    #[cfg(feature = "coherence-bonus-fusion")]
    fn test_resonance_bonus_invalid_signals_excluded_from_coherence() {
        let doc = SearchResult {
            id: "doc1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        };

        let set_v = ("vector".to_string(), vec![doc.clone()], 1.0);
        let set_t = ("text".to_string(), vec![doc.clone()], 1.0);
        let set_g = ("graph".to_string(), vec![doc.clone()], 1.0);
        let set_invalid = ("invalid".to_string(), vec![doc.clone()], f32::NAN);

        let cfg = ResonanceConfig {
            beta: 0.5,
            gamma: 0.3,
        };

        // 4 signals input, 1 invalid -> valid_signal_count = 3.
        // doc1 is in all 3 valid signals -> coherence = (3/3)^0.5 = 1.0 -> coherence_bonus = 0.3 * 1.0 = 0.3
        let fused = weighted_reciprocal_rank_fusion_with_options(
            vec![set_v, set_t, set_g, set_invalid],
            10,
            MetadataMergePriority::default(),
            true,
            Some(&cfg),
        );

        assert_eq!(fused.len(), 1);
        let prov = match fused[0].provenance.as_ref() {
            Some(p) => p,
            None => panic!("provenance present"),
        };
        assert!(
            (prov.coherence_bonus - 0.3).abs() < 1e-6,
            "coherence_bonus should be 0.3 for full coherence over valid signals, got {}",
            prov.coherence_bonus
        );
    }

    #[test]
    #[cfg(feature = "coherence-bonus-fusion")]
    fn test_resonance_bonus_single_signal_no_boost() {
        let set1 = (
            "vector".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );

        let cfg = ResonanceConfig {
            beta: 0.5,
            gamma: 0.3,
        };
        let fused = weighted_reciprocal_rank_fusion_with_options(
            vec![set1],
            10,
            MetadataMergePriority::default(),
            true,
            Some(&cfg),
        );

        assert_eq!(fused.len(), 1);
        let prov = match fused[0].provenance.as_ref() {
            Some(p) => p,
            None => panic!("provenance present"),
        };
        assert!((prov.coherence_bonus - 0.3).abs() < 1e-6);
    }

    #[test]
    #[cfg(feature = "coherence-bonus-fusion")]
    fn test_apply_resonance_bonus_handles_nan_score_deterministically() {
        let results = vec![
            SearchResult {
                id: "doc_mid".to_string(),
                score: 0.5,
                metadata: None,
                matched_signals: vec!["vector".to_string()],
                provenance: None,
            },
            SearchResult {
                id: "doc_nan".to_string(),
                score: f32::NAN,
                metadata: None,
                matched_signals: vec!["vector".to_string(), "text".to_string()],
                provenance: None,
            },
            SearchResult {
                id: "doc_top".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec!["vector".to_string()],
                provenance: None,
            },
        ];

        let cfg = ResonanceConfig::default();
        let first_run = apply_resonance_bonus(results.clone(), 2, &cfg);

        // Verify loop determinism over 100 iterations
        for _ in 0..100 {
            let run = apply_resonance_bonus(results.clone(), 2, &cfg);
            assert_eq!(run.len(), first_run.len());
            for (r1, r2) in run.iter().zip(first_run.iter()) {
                assert_eq!(r1.id, r2.id);
                if r1.score.is_nan() {
                    assert!(r2.score.is_nan());
                } else {
                    assert_eq!(r1.score, r2.score);
                }
            }
        }

        // NC-6: NaN-Score muss am ENDE der sortierten Liste landen (nicht vorne)
        assert_eq!(first_run.len(), 3);
        assert_ne!(first_run[0].id, "doc_nan", "NaN-Score darf nicht an Listenspitze stehen");
        assert_eq!(first_run[0].id, "doc_top");
        assert_eq!(first_run[1].id, "doc_mid");
        assert_eq!(first_run.last().map(|r| r.id.as_str()), Some("doc_nan"));
        assert!(first_run.last().map_or(false, |r| r.score.is_nan()));
    }

    #[test]
    #[cfg(feature = "coherence-bonus-fusion")]
    fn test_apply_resonance_bonus_finite_scores_regression() {
        let results = vec![
            SearchResult {
                id: "doc_single_signal_low".to_string(),
                score: 0.2,
                metadata: None,
                matched_signals: vec!["vector".to_string()],
                provenance: None,
            },
            SearchResult {
                id: "doc_two_signals_mid".to_string(),
                score: 0.5,
                metadata: None,
                matched_signals: vec!["vector".to_string(), "text".to_string()],
                provenance: None,
            },
            SearchResult {
                id: "doc_three_signals_high".to_string(),
                score: 0.8,
                metadata: None,
                matched_signals: vec![
                    "vector".to_string(),
                    "text".to_string(),
                    "graph".to_string(),
                ],
                provenance: None,
            },
            SearchResult {
                id: "doc_no_signals".to_string(),
                score: 0.6,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "doc_all_signals".to_string(),
                score: 0.4,
                metadata: None,
                matched_signals: vec![
                    "vector".to_string(),
                    "text".to_string(),
                    "graph".to_string(),
                    "custom".to_string(),
                ],
                provenance: None,
            },
        ];

        let cfg = ResonanceConfig {
            beta: 0.5,
            gamma: 0.3,
        };
        let total_signals = 4;
        let boosted = apply_resonance_bonus(results, total_signals, &cfg);

        // Expected coherence calculations:
        // total_signals = 4
        // beta = 0.5, gamma = 0.3
        // doc_three_signals_high: 0.8 * (1.0 + 0.3 * (3/4)^0.5) = 0.8 * (1.0 + 0.3 * 0.8660254) = 0.8 * 1.2598076 = 1.0078461
        // doc_all_signals:        0.4 * (1.0 + 0.3 * (4/4)^0.5) = 0.4 * (1.0 + 0.3 * 1.0) = 0.4 * 1.3 = 0.52
        // doc_two_signals_mid:   0.5 * (1.0 + 0.3 * (2/4)^0.5) = 0.5 * (1.0 + 0.3 * 0.7071068) = 0.5 * 1.212132 = 0.606066
        // doc_no_signals:        0.6 * (1.0 + 0.3 * (0/4)^0.5) = 0.6 * 1.0 = 0.6
        // doc_single_signal_low: 0.2 * (1.0 + 0.3 * (1/4)^0.5) = 0.2 * (1.0 + 0.3 * 0.5) = 0.2 * 1.15 = 0.23

        let ids: Vec<&str> = boosted.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "doc_three_signals_high",
                "doc_two_signals_mid",
                "doc_no_signals",
                "doc_all_signals",
                "doc_single_signal_low"
            ]
        );

        // Verify exact boosted score calculations
        let score_map: std::collections::HashMap<&str, f32> =
            boosted.iter().map(|r| (r.id.as_str(), r.score)).collect();

        assert!((score_map["doc_three_signals_high"] - 1.0078461).abs() < 1e-5);
        assert!((score_map["doc_two_signals_mid"] - 0.606066).abs() < 1e-5);
        assert!((score_map["doc_no_signals"] - 0.6).abs() < 1e-5);
        assert!((score_map["doc_all_signals"] - 0.52).abs() < 1e-5);
        assert!((score_map["doc_single_signal_low"] - 0.23).abs() < 1e-5);
    }

    #[test]
    #[cfg(feature = "coherence-bonus-fusion")]
    fn test_inv_prov2_signal_contributions_unchanged() {
        let set1 = (
            "vector".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );
        let set2 = (
            "text".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.8,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            }],
            1.0,
        );

        let cfg = ResonanceConfig {
            beta: 0.5,
            gamma: 0.3,
        };
        let fused = weighted_reciprocal_rank_fusion_with_options(
            vec![set1, set2],
            10,
            MetadataMergePriority::default(),
            true,
            Some(&cfg),
        );

        let res = &fused[0];
        let prov = match res.provenance.as_ref() {
            Some(p) => p,
            None => panic!("provenance present"),
        };

        let unboosted_sum: f32 = prov
            .signal_contributions
            .values()
            .map(|c| c.rrf_contribution)
            .sum();

        let expected_unboosted = (1.0 / 61.0) + (1.0 / 61.0);
        assert!((unboosted_sum - expected_unboosted).abs() < 1e-6);

        let expected_final = expected_unboosted * (1.0 + prov.coherence_bonus);
        assert!((res.score - expected_final).abs() < 1e-6);
        assert!((unboosted_sum - res.score).abs() > 1e-6);
    }

    #[test]
    fn test_rrf_fusion_rejects_nan_weight_without_score_corruption() {
        let set1 = vec![
            SearchResult {
                id: "doc1".to_string(),
                score: 0.9,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "doc2".to_string(),
                score: 0.8,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        ];
        let set2_nan = vec![
            SearchResult {
                id: "doc1".to_string(),
                score: 0.95,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
            SearchResult {
                id: "doc3".to_string(),
                score: 0.7,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            },
        ];
        let set3 = vec![SearchResult {
            id: "doc2".to_string(),
            score: 0.85,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        }];

        let result_with_nan = weighted_reciprocal_rank_fusion_with_options(
            vec![
                ("vector".to_string(), set1.clone(), 1.0),
                ("text".to_string(), set2_nan.clone(), f32::NAN),
                ("graph".to_string(), set3.clone(), 0.5),
            ],
            10,
            MetadataMergePriority::default(),
            true,
            None,
        );

        assert!(
            result_with_nan.iter().all(|r| r.score.is_finite()),
            "No score in fusion results should be NaN or non-finite"
        );

        let result_without_nan_signal = weighted_reciprocal_rank_fusion_with_options(
            vec![
                ("vector".to_string(), set1, 1.0),
                ("graph".to_string(), set3, 0.5),
            ],
            10,
            MetadataMergePriority::default(),
            true,
            None,
        );

        assert_eq!(
            result_with_nan.len(),
            result_without_nan_signal.len(),
            "Signal with NaN weight must produce identical result count"
        );
        for (r1, r2) in result_with_nan.iter().zip(result_without_nan_signal.iter()) {
            assert_eq!(r1.id, r2.id);
            assert_eq!(r1.score, r2.score);
            assert_eq!(r1.metadata, r2.metadata);
            assert_eq!(r1.matched_signals, r2.matched_signals);
        }
    }

    #[test]
    fn test_rrf_fusion_logs_nonfinite_raw_score() {
        let set_with_nan_raw_score = vec![SearchResult {
            id: "doc1".to_string(),
            score: f32::NAN,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        }];

        let result = weighted_reciprocal_rank_fusion_with_options(
            vec![("vector".to_string(), set_with_nan_raw_score, 1.0)],
            10,
            MetadataMergePriority::default(),
            true,
            None,
        );

        assert_eq!(result.len(), 1);
        assert!(
            result[0].score.is_finite(),
            "RRF rank score must remain finite despite NaN raw doc score"
        );

        let prov = result[0]
            .provenance
            .as_ref()
            .expect("provenance should be attached");
        assert!(
            prov.vector_distance.is_some_and(|s| s.is_nan()),
            "Non-finite raw score should be preserved as NaN in vector_distance provenance for traceabilty"
        );
        let contrib = prov
            .signal_contributions
            .get("vector")
            .expect("vector signal contribution present");
        assert!(
            contrib.raw_score.is_nan(),
            "Non-finite raw score should be preserved as NaN in signal contribution raw_score"
        );
    }

    #[test]
    fn test_provenance_attribution_sums_to_rrf() {
        let vec_set = (
            "vector".to_string(),
            vec![
                SearchResult {
                    id: "doc1".to_string(),
                    score: 0.95,
                    metadata: None,
                    matched_signals: vec![],
                    provenance: None,
                },
                SearchResult {
                    id: "doc2".to_string(),
                    score: 0.85,
                    metadata: None,
                    matched_signals: vec![],
                    provenance: None,
                },
            ],
            1.0,
        );
        let text_set = (
            "text".to_string(),
            vec![
                SearchResult {
                    id: "doc2".to_string(),
                    score: 4.2,
                    metadata: None,
                    matched_signals: vec![],
                    provenance: None,
                },
                SearchResult {
                    id: "doc1".to_string(),
                    score: 3.1,
                    metadata: None,
                    matched_signals: vec![],
                    provenance: None,
                },
            ],
            0.8,
        );
        let graph_set = (
            "graph".to_string(),
            vec![SearchResult {
                id: "doc1".to_string(),
                score: 0.7,
                metadata: None,
                matched_signals: vec![],
                provenance: None,
            }],
            0.5,
        );

        let fused = weighted_reciprocal_rank_fusion(vec![vec_set, text_set, graph_set], 10);
        assert_eq!(fused.len(), 2);

        for res in &fused {
            let prov = match res.provenance.as_ref() {
                Some(p) => p,
                None => panic!("Provenance must be present"),
            };
            assert!(!prov.signal_contributions.is_empty());
            let sum_contrib: f32 = prov
                .signal_contributions
                .values()
                .map(|c| c.rrf_contribution)
                .sum();
            assert!(
                (sum_contrib - res.score).abs() < 1e-6,
                "INV-PROV-1 violation: sum of contributions {sum_contrib} != final score {}",
                res.score
            );
        }
    }

    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_rrf_never_panics(
                result_sets in prop::collection::vec(
                    prop::collection::vec(
                        any::<u32>().prop_map(|i| SearchResult {
                            id: format!("doc_{}", i % 100), // Collisions are good for testing
                            score: 0.0,
                            metadata: None,
                            matched_signals: vec![],
                            provenance: None,
                        }),
                        0..20
                    ),
                    0..5
                ),
                max_results in 0..50usize
            ) {
                let fused = reciprocal_rank_fusion(result_sets, max_results);
                assert!(fused.len() <= max_results);
            }

            #[test]
            fn prop_rrf_score_monotonicity(
                rank in 0..10usize
            ) {
                let rrf_top = 1.0 / (60.0 + 1.0);
                let rrf_low = 1.0 / (60.0 + (rank + 1) as f32);
                assert!(rrf_top >= rrf_low);
            }
        }
    }
}
