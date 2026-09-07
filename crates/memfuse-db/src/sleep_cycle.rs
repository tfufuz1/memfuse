//! REM-Phase sleep cycle processing for community synthesis and abstraction.
//!
//! Provides deterministic community stability tracking and LLM-driven meta-chunk synthesis.

// FILE-CONTEXT
// STAND: 2026-09-07T00:00:00Z
// ZWECK: REM-Phase Sleep-Cycle Consolidation & MetaChunk Synthesis
// INVARIANTEN: No unwrap/panic in production code; abstracts_from.len() >= 1; max_llm_calls_per_cycle strictly enforced.

use memfuse_core::{DocId, LlmTextGenerator, Result, TxId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Configuration for the REM phase of sleep cycle processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemConfig {
    /// Minimum community member count required for synthesis. Default: 4.
    pub min_community_size: usize,
    /// Cohesion threshold for community qualification. Default: 0.6.
    pub cohesion_threshold: f32,
    /// Number of consecutive sleep cycle runs a community must be stably observed. Default: 3.
    pub stability_cycles_required: u32,
    /// Maximum number of LLM synthesis calls allowed per cycle (cost guardrail P12). Default: 10.
    pub max_llm_calls_per_cycle: u32,
}

impl Default for RemConfig {
    fn default() -> Self {
        Self {
            min_community_size: 4,
            cohesion_threshold: 0.6,
            stability_cycles_required: 3,
            max_llm_calls_per_cycle: 10,
        }
    }
}

/// Tracks community stability over consecutive sleep cycles.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommunityStabilityTracker {
    history: HashMap<u64, u32>,
}

impl CommunityStabilityTracker {
    /// Creates a new, empty `CommunityStabilityTracker`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Observes a community hash in the current cycle, incrementing its consecutive stability count.
    /// Returns the updated stability count.
    pub fn observe(&mut self, community_members_hash: u64) -> u32 {
        let count = self.history.entry(community_members_hash).or_insert(0);
        *count += 1;
        *count
    }

    /// Removes communities from history if they were not observed in the current cycle.
    pub fn reset_if_absent(&mut self, currently_observed: &HashSet<u64>) {
        self.history.retain(|k, _| currently_observed.contains(k));
    }
}

/// Synthesized high-level context chunk abstracting multiple source documents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetaChunk {
    /// Synthesized text content prefixed with machine-readable source count marker.
    pub content: String,
    /// Source document identifiers abstracted by this MetaChunk (MANDATORY: len >= 1).
    pub abstracts_from: Vec<DocId>,
    /// Unique hash identifier of the source community.
    pub source_community_hash: u64,
    /// Transaction ID at which this MetaChunk was created.
    pub created_at_tx: TxId,
    /// Identifier of the LLM model used for synthesis.
    pub llm_model_id: String,
}

/// Result of running the REM phase across stable communities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemPhaseResult {
    /// Synthesized meta-chunks produced in this cycle.
    pub synthesized: Vec<MetaChunk>,
    /// Hashes of qualified communities deferred to future cycles due to `max_llm_calls_per_cycle` budget limit.
    pub deferred_community_hashes: Vec<u64>,
}

/// Executes the REM phase across detected stable communities.
///
/// Synthesizes high-level context chunks (`MetaChunk`) for qualified communities up to
/// `config.max_llm_calls_per_cycle`. Excess communities are deferred without failing the phase.
pub async fn run_rem_phase(
    stable_communities: &[(u64, Vec<DocId>)],
    source_texts: &HashMap<DocId, String>,
    llm: &(impl LlmTextGenerator + ?Sized),
    config: &RemConfig,
) -> Result<RemPhaseResult> {
    run_rem_phase_with_tx(
        stable_communities,
        source_texts,
        llm,
        config,
        TxId(0),
        "default",
    )
    .await
}

/// Executes the REM phase with explicit transaction ID and model identifier metadata.
pub async fn run_rem_phase_with_tx(
    stable_communities: &[(u64, Vec<DocId>)],
    source_texts: &HashMap<DocId, String>,
    llm: &(impl LlmTextGenerator + ?Sized),
    config: &RemConfig,
    created_at_tx: TxId,
    llm_model_id: &str,
) -> Result<RemPhaseResult> {
    // 1. Filter stable communities by minimum community size
    let qualified: Vec<&(u64, Vec<DocId>)> = stable_communities
        .iter()
        .filter(|(_, members)| members.len() >= config.min_community_size)
        .collect();

    let max_calls = config.max_llm_calls_per_cycle as usize;
    let (to_process, deferred) = if qualified.len() > max_calls {
        qualified.split_at(max_calls)
    } else {
        (qualified.as_slice(), [].as_slice())
    };

    let deferred_community_hashes: Vec<u64> = deferred.iter().map(|(hash, _)| *hash).collect();
    let mut synthesized = Vec::with_capacity(to_process.len());

    for (community_hash, members) in to_process {
        if members.is_empty() {
            tracing::warn!(
                community_hash = community_hash,
                "Skipping community synthesis for empty member set"
            );
            continue;
        }

        let mut prompt_content = String::new();
        for doc_id in members {
            if let Some(text) = source_texts.get(doc_id) {
                prompt_content.push_str(&format!("- Chunk [DocId: {}]: {}\n", doc_id.0, text));
            }
        }

        let prompt = format!(
            "Synthesisiere die folgenden Dokumenten-Texte zu einem kohärenten Meta-Kontext:\n\n{}\n\nSynthese:",
            prompt_content
        );

        match llm.generate(&prompt).await {
            Ok(summary) => {
                let content = format!(
                    "[SYNTHESIZED FROM {} SOURCES]\n{}",
                    members.len(),
                    summary
                );
                synthesized.push(MetaChunk {
                    content,
                    abstracts_from: members.clone(),
                    source_community_hash: *community_hash,
                    created_at_tx,
                    llm_model_id: llm_model_id.to_string(),
                });
            }
            Err(e) => {
                tracing::error!(
                    community_hash = community_hash,
                    error = %e,
                    "LLM synthesis failed for community; continuing with remaining communities"
                );
            }
        }
    }

    Ok(RemPhaseResult {
        synthesized,
        deferred_community_hashes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::BoxFuture;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct MockLlmGenerator {
        fail_community_contains: Option<String>,
        call_count: Arc<AtomicU32>,
    }

    impl LlmTextGenerator for MockLlmGenerator {
        fn generate<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String>> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let prompt_owned = prompt.to_string();
            let fail_pattern = self.fail_community_contains.clone();
            Box::pin(async move {
                if let Some(pattern) = fail_pattern {
                    if prompt_owned.contains(&pattern) {
                        return Err(memfuse_core::MemFuseError::Internal(
                            "Simulated LLM synthesis failure".to_string(),
                        ));
                    }
                }
                Ok(format!(
                    "Synthesized summary for prompt len {}",
                    prompt_owned.len()
                ))
            })
        }
    }

    #[tokio::test]
    async fn test_community_below_min_size_skipped() {
        let config = RemConfig {
            min_community_size: 4,
            ..Default::default()
        };
        let llm = MockLlmGenerator::default();

        let stable_communities = vec![
            (1001, vec![DocId(1), DocId(2), DocId(3)]), // size 3 < min 4
            (1002, vec![DocId(4), DocId(5), DocId(6), DocId(7)]), // size 4 >= min 4
        ];

        let mut source_texts = HashMap::new();
        for id in 1..=7 {
            source_texts.insert(DocId(id), format!("Text for doc {}", id));
        }

        let res = run_rem_phase(&stable_communities, &source_texts, &llm, &config)
            .await
            .unwrap();

        assert_eq!(res.synthesized.len(), 1);
        assert_eq!(res.synthesized[0].source_community_hash, 1002);
        assert_eq!(res.synthesized[0].abstracts_from.len(), 4);
        assert!(res.deferred_community_hashes.is_empty());
        assert_eq!(llm.call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_community_stability_tracker_observation_and_reset() {
        let mut tracker = CommunityStabilityTracker::new();
        let comm_hash_a = 0xA1B2C3D4;
        let comm_hash_b = 0xE5F67890;

        assert_eq!(tracker.observe(comm_hash_a), 1);
        assert_eq!(tracker.observe(comm_hash_a), 2);
        // 2 cycles observed < required 3
        assert_eq!(tracker.history.get(&comm_hash_a), Some(&2));

        assert_eq!(tracker.observe(comm_hash_b), 1);

        // Current observation only includes comm_hash_a
        let mut observed = HashSet::new();
        observed.insert(comm_hash_a);

        tracker.reset_if_absent(&observed);

        assert_eq!(tracker.history.get(&comm_hash_a), Some(&2));
        assert_eq!(tracker.history.get(&comm_hash_b), None);
    }

    #[tokio::test]
    async fn test_max_llm_calls_per_cycle_limit() {
        let config = RemConfig {
            min_community_size: 1,
            max_llm_calls_per_cycle: 10,
            ..Default::default()
        };
        let llm = MockLlmGenerator::default();

        let mut stable_communities = Vec::new();
        let mut source_texts = HashMap::new();

        for i in 1..=15 {
            let hash = i as u64;
            let doc_id = DocId(i as u64);
            stable_communities.push((hash, vec![doc_id]));
            source_texts.insert(doc_id, format!("Doc text {}", i));
        }

        let res = run_rem_phase(&stable_communities, &source_texts, &llm, &config)
            .await
            .unwrap();

        assert_eq!(res.synthesized.len(), 10);
        assert_eq!(res.deferred_community_hashes.len(), 5);
        assert_eq!(res.deferred_community_hashes, vec![11, 12, 13, 14, 15]);
        assert_eq!(llm.call_count.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn test_meta_chunk_mandatory_source_and_synthesized_marker() {
        let config = RemConfig {
            min_community_size: 2,
            ..Default::default()
        };
        let llm = MockLlmGenerator::default();

        let stable_communities = vec![(9999, vec![DocId(10), DocId(11)])];
        let mut source_texts = HashMap::new();
        source_texts.insert(DocId(10), "Source doc 10 content".to_string());
        source_texts.insert(DocId(11), "Source doc 11 content".to_string());

        let res = run_rem_phase_with_tx(
            &stable_communities,
            &source_texts,
            &llm,
            &config,
            TxId(42),
            "mock-llm-v1",
        )
        .await
        .unwrap();

        assert_eq!(res.synthesized.len(), 1);
        let meta = &res.synthesized[0];

        assert!(
            !meta.abstracts_from.is_empty(),
            "abstracts_from MUST have >= 1 source"
        );
        assert_eq!(meta.abstracts_from.len(), 2);
        assert_eq!(meta.created_at_tx, TxId(42));
        assert_eq!(meta.llm_model_id, "mock-llm-v1");
        assert!(
            meta.content.starts_with("[SYNTHESIZED FROM 2 SOURCES]"),
            "Content must start with machine-readable marker prefix"
        );
    }

    #[tokio::test]
    async fn test_llm_failure_resilience() {
        let config = RemConfig {
            min_community_size: 1,
            max_llm_calls_per_cycle: 10,
            ..Default::default()
        };

        // Fail when processing community 2's doc text
        let llm = MockLlmGenerator {
            fail_community_contains: Some("Doc text 2".to_string()),
            call_count: Arc::new(AtomicU32::new(0)),
        };

        let stable_communities = vec![
            (101, vec![DocId(1)]),
            (102, vec![DocId(2)]), // This one will fail LLM generation
            (103, vec![DocId(3)]),
        ];

        let mut source_texts = HashMap::new();
        source_texts.insert(DocId(1), "Doc text 1".to_string());
        source_texts.insert(DocId(2), "Doc text 2".to_string());
        source_texts.insert(DocId(3), "Doc text 3".to_string());

        let res = run_rem_phase(&stable_communities, &source_texts, &llm, &config)
            .await
            .unwrap();

        // Failed community 102 should be skipped without breaking overall phase
        assert_eq!(res.synthesized.len(), 2);
        let synthesized_hashes: Vec<u64> = res
            .synthesized
            .iter()
            .map(|m| m.source_community_hash)
            .collect();
        assert_eq!(synthesized_hashes, vec![101, 103]);
    }
}
