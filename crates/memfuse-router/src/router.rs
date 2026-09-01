//! Core routing engine for matching hybrid search context to SLM profiles.

use crate::profile::SlmProfile;
use memfuse_core::{ContextChunk, ContextWindow, EntityId, MemFuseError, Result};
use memfuse_db::{collection::Collection, context::ContextManager};
use memfuse_store::LsmStorage;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Result of a routing operation containing the selected profile and prepared context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// The target SLM profile selected for the query.
    pub profile: SlmProfile,
    /// The trimmed context window prepared specifically for the selected profile's token budget.
    pub context: ContextWindow,
}

/// Router engine that routes queries to optimal SLM backends based on community assignment and search scores.
pub struct RouterEngine {
    collection: Arc<Collection<LsmStorage>>,
    profiles: RwLock<Vec<SlmProfile>>,
}

impl RouterEngine {
    /// Creates a new `RouterEngine` instance.
    pub fn new(collection: Arc<Collection<LsmStorage>>, profiles: Vec<SlmProfile>) -> Self {
        Self {
            collection,
            profiles: RwLock::new(profiles),
        }
    }

    /// Dynamically updates configured SLM profiles at runtime (Hot-Reload).
    pub fn update_profiles(&self, new_profiles: Vec<SlmProfile>) {
        *self.profiles.write() = new_profiles;
    }

    /// Returns a copy of the active SLM profiles.
    pub fn profiles(&self) -> Vec<SlmProfile> {
        self.profiles.read().clone()
    }

    /// Routes a query with embedding and text to the best matching SLM profile.
    #[allow(deprecated)]
    pub async fn route(
        &self,
        query_embedding: &[f32],
        query_text: &str,
    ) -> Result<RoutingDecision> {
        // Snapshot profiles atomically to guarantee caller consistency during hot-reloads
        let profiles = self.profiles.read().clone();

        if profiles.is_empty() {
            return Err(MemFuseError::NotFound(
                "Keine SLM-Profile für Routing konfiguriert".to_string(),
            ));
        }

        // 1. Perform hybrid search with standard fusion weights
        let search_results = self
            .collection
            .hybrid_search_with_strategy(query_text, query_embedding, 10, None, None, None, None)
            .await?;

        if search_results.is_empty() {
            return Err(MemFuseError::NotFound(
                "Keine relevanten Suchergebnisse für Routing gefunden".to_string(),
            ));
        }

        // 2. Identify communities and score candidate profiles
        // Convert search results into ContextChunks first using TryFrom / ContextChunk construction
        let mut chunks: Vec<(ContextChunk, Option<u64>)> = Vec::new();

        for res in &search_results {
            let chunk_res = res.clone();
            if let Ok(mut chunk) = ContextChunk::try_from(chunk_res) {
                // Determine community ID if entity_id can be parsed
                let comm_id = if let Ok(eid) = EntityId::from_key(&res.id) {
                    self.collection.get_community(eid).await.ok().flatten()
                } else {
                    None
                };

                // Ensure content uses ContextChunk::combined_text_owned() for context preparation
                chunk.content = chunk.combined_text_owned();
                chunks.push((chunk, comm_id));
            }
        }

        // 3. Select profile with highest aggregated relevance score
        let selected_profile_idx = select_profile_from_chunks(&profiles, &chunks)?;
        let selected_profile = profiles[selected_profile_idx].clone();

        // 4. Construct ContextWindow using ContextManager tailored to selected_profile.token_budget
        let raw_chunks: Vec<ContextChunk> = chunks.into_iter().map(|(c, _)| c).collect();
        let mut context_mgr = ContextManager::new(selected_profile.token_budget.clone());
        context_mgr.set_relevance_threshold(0.0);
        let context_window = context_mgr.prepare_context(raw_chunks)?;

        Ok(RoutingDecision {
            profile: selected_profile,
            context: context_window,
        })
    }
}

/// Computes the NaN-safe maximum score across chunks for a given profile.
///
/// Filters out non-finite scores (`NaN` and `Inf`) before performing the fold aggregation.
pub(crate) fn compute_max_score(
    profile: &SlmProfile,
    chunks: &[(ContextChunk, Option<u64>)],
) -> f32 {
    chunks
        .iter()
        .map(|(c, c_id)| {
            let mut s = c.relevance;
            if let Some(id) = c_id {
                if profile.domain_communities.contains(id) {
                    s *= 1.2;
                }
            }
            s
        })
        .filter(|score| score.is_finite())
        .fold(0.0f32, f32::max)
}

/// Selects the best matching SLM profile index based on candidate chunks and domain community matches.
///
/// # Errors
/// Returns `MemFuseError::NotFound` if:
/// - `chunks` is empty.
/// - All chunk relevance scores are non-finite (`NaN`/`Inf`), indicating possible upstream corruption.
/// - No candidate SLM profile satisfies the minimum relevance threshold or community match.
pub(crate) fn select_profile_from_chunks(
    profiles: &[SlmProfile],
    chunks: &[(ContextChunk, Option<u64>)],
) -> Result<usize> {
    if chunks.is_empty() {
        return Err(MemFuseError::NotFound(
            "Keine gültigen Chunks aus Suchergebnissen ermittelbar".to_string(),
        ));
    }

    // Explicitly check for upstream distance corruption where all relevance scores are NaN/Inf
    if !chunks.iter().any(|(c, _)| c.relevance.is_finite()) {
        tracing::error!(
            "Alle Chunk-Relevanzwerte sind NaN/Inf — mögliche Upstream-Korruption in der Distanzberechnung"
        );
        return Err(MemFuseError::NotFound(
            "Alle Chunk-Relevanzwerte sind NaN/Inf — mögliche Upstream-Korruption in der Distanzberechnung".to_string(),
        ));
    }

    let mut profile_scores: HashMap<usize, f32> = HashMap::new();

    for (idx, profile) in profiles.iter().enumerate() {
        let mut aggregated_score = 0.0f32;
        let mut matched_community = false;

        for (chunk, comm_id) in chunks {
            if !chunk.relevance.is_finite() {
                continue;
            }
            let mut score = chunk.relevance;
            if let Some(c_id) = comm_id {
                if profile.domain_communities.contains(c_id) {
                    score *= 1.2;
                    matched_community = true;
                }
            }
            aggregated_score += score;
        }

        let max_score = compute_max_score(profile, chunks);

        if matched_community
            && (aggregated_score >= profile.min_relevance_score
                || max_score >= profile.min_relevance_score)
        {
            profile_scores.insert(idx, aggregated_score);
        }
    }

    let best_profile_idx = profile_scores
        .into_iter()
        .max_by(|(idx_a, score_a), (idx_b, score_b)| {
            score_a.total_cmp(score_b).then_with(|| idx_b.cmp(idx_a))
        })
        .map(|(idx, _)| idx);

    match best_profile_idx {
        Some(idx) => Ok(idx),
        None => Err(MemFuseError::NotFound(
            "Kein SLM-Profil erreicht den erforderlichen min_relevance_score oder die Community-Zuordnung".to_string(),
        )),
    }
}
