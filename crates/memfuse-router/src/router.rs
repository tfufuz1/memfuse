//! Core routing engine for matching hybrid search context to SLM profiles.

use crate::profile::{ProfileCalibrationState, SlmProfile};
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
    calibration: RwLock<HashMap<String, ProfileCalibrationState>>,
}

impl RouterEngine {
    /// Creates a new `RouterEngine` instance.
    pub fn new(collection: Arc<Collection<LsmStorage>>, profiles: Vec<SlmProfile>) -> Self {
        let calibration: HashMap<String, ProfileCalibrationState> = profiles
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    ProfileCalibrationState::new(p.min_relevance_score),
                )
            })
            .collect();
        Self {
            collection,
            profiles: RwLock::new(profiles),
            calibration: RwLock::new(calibration),
        }
    }

    /// Validates all profiles and creates a new `RouterEngine` instance.
    pub fn try_new(
        collection: Arc<Collection<LsmStorage>>,
        profiles: Vec<SlmProfile>,
    ) -> Result<Self> {
        for p in &profiles {
            p.validate()?;
        }
        Ok(Self::new(collection, profiles))
    }

    /// Dynamically updates configured SLM profiles at runtime (Hot-Reload).
    pub fn update_profiles(&self, new_profiles: Vec<SlmProfile>) {
        let mut cal = self.calibration.write();
        let new_cal: HashMap<String, ProfileCalibrationState> = new_profiles
            .iter()
            .map(|p| {
                let state = cal
                    .remove(&p.name)
                    .unwrap_or_else(|| ProfileCalibrationState::new(p.min_relevance_score));
                (p.name.clone(), state)
            })
            .collect();
        *cal = new_cal;
        *self.profiles.write() = new_profiles;
    }

    /// Validates all profiles and updates configured SLM profiles at runtime (Hot-Reload).
    pub fn try_update_profiles(&self, new_profiles: Vec<SlmProfile>) -> Result<()> {
        for p in &new_profiles {
            p.validate()?;
        }
        self.update_profiles(new_profiles);
        Ok(())
    }

    /// Returns a copy of the active SLM profiles.
    pub fn profiles(&self) -> Vec<SlmProfile> {
        self.profiles.read().clone()
    }

    /// Gibt aktuelle Kalibrierungsstatistik für alle Profile zurück.
    pub fn calibration_stats(&self) -> HashMap<String, ProfileCalibrationState> {
        self.calibration.read().clone()
    }

    /// Setzt Kalibrierungsstatistik für ein bestimmtes Profil zurück.
    pub fn reset_calibration(&self, profile_name: &str) {
        if let Some(state) = self.calibration.write().get_mut(profile_name) {
            state.reset();
        }
    }

    /// Setzt Kalibrierungsstatistik für alle Profile zurück.
    pub fn reset_all_calibration(&self) {
        for state in self.calibration.write().values_mut() {
            state.reset();
        }
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
                // Determine community ID directly from chunk.doc_id (derived from res.id in TryFrom)
                let eid = EntityId::from_doc_id(chunk.doc_id);
                let comm_id = self.collection.get_community(eid).await.ok().flatten();

                // Ensure content uses ContextChunk::combined_text_owned() for context preparation
                chunk.content = chunk.combined_text_owned();
                chunks.push((chunk, comm_id));
            }
        }

        // 3. Select profile with highest aggregated relevance score
        let profile_scores = compute_profile_scores(&profiles, &chunks);

        let (selected_profile_idx, selected_profile) = match select_profile_from_chunks(
            &profiles, &chunks,
        ) {
            Ok(idx) => (idx, profiles[idx].clone()),
            Err(MemFuseError::NotFound(ref msg)) if msg.contains("min_relevance_score") => {
                // Kaskaden-Fallback: Profil mit niedrigstem min_relevance_score
                // (oder kalibriertem calibrated_min_score) als Notfall-Profil
                let cal = self.calibration.read();
                let fallback_idx = profiles
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let score_a = cal
                            .get(&a.name)
                            .map(|s| s.calibrated_min_score)
                            .unwrap_or(a.min_relevance_score);
                        let score_b = cal
                            .get(&b.name)
                            .map(|s| s.calibrated_min_score)
                            .unwrap_or(b.min_relevance_score);
                        score_a.total_cmp(&score_b)
                    })
                    .map(|(idx, _)| idx)
                    .ok_or_else(|| {
                        MemFuseError::NotFound("Keine SLM-Profile konfiguriert".to_string())
                    })?;

                tracing::warn!(
                    profile = %profiles[fallback_idx].name,
                    "Kaskaden-Fallback: Kein Profil über Schwellenwert, nutze Profil mit niedrigstem min_relevance_score"
                );
                (fallback_idx, profiles[fallback_idx].clone())
            }
            Err(other) => return Err(other),
        };

        // Kalibrierungs-Tracking: Konfidenz berechnen
        {
            let best_score = profile_scores
                .get(&selected_profile_idx)
                .copied()
                .unwrap_or(0.0);
            let second_best = profile_scores
                .iter()
                .filter(|(idx, _)| **idx != selected_profile_idx)
                .map(|(_, s)| *s)
                .fold(0.0f32, f32::max);
            let confidence = if second_best > 0.0 {
                (best_score / second_best) as f64
            } else {
                2.0 // Nur ein Kandidat → hohe Konfidenz
            };

            let mut cal = self.calibration.write();
            if let Some(state) = cal.get_mut(&selected_profile.name) {
                state.times_selected += 1;
                state.cumulative_confidence += confidence;
                // Rekalibrierung versuchen (alle 10 Entscheidungen)
                if state.times_selected % 10 == 0 {
                    state.recalibrate(0.7); // Schwellenwert: 70% Konfidenz
                }
            }
        }

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

/// Computes candidate scores for all profiles across chunks.
pub(crate) fn compute_profile_scores(
    profiles: &[SlmProfile],
    chunks: &[(ContextChunk, Option<u64>)],
) -> HashMap<usize, f32> {
    let mut profile_scores: HashMap<usize, f32> = HashMap::new();

    for (idx, profile) in profiles.iter().enumerate() {
        let mut aggregated_score = 0.0f32;

        for (chunk, comm_id) in chunks {
            if !chunk.relevance.is_finite() {
                continue;
            }
            let mut score = chunk.relevance;
            if let Some(c_id) = comm_id {
                if profile.domain_communities.contains(c_id) {
                    score *= 1.2;
                }
            }
            aggregated_score += score;
        }

        profile_scores.insert(idx, aggregated_score);
    }

    profile_scores
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
    let mut any_community_matched = false;

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
                    any_community_matched = true;
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
        None => {
            if any_community_matched {
                Err(MemFuseError::NotFound(
                    "Kein SLM-Profil erreicht den erforderlichen min_relevance_score".to_string(),
                ))
            } else {
                Err(MemFuseError::NotFound(
                    "Kein SLM-Profil entspricht der Community-Zuordnung".to_string(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::TokenBudget;

    #[tokio::test]
    async fn test_calibration_stats_initial_state() {
        let dir = tempfile::tempdir().unwrap();
        let config = memfuse_db::MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = memfuse_db::MemFuse::open_with_config(dir.path(), config)
            .await
            .unwrap();
        let collection = db.collection("default").await.unwrap();

        let profile1 = SlmProfile::new(
            "p1",
            "http://localhost:1111",
            vec![1],
            TokenBudget::new(1000, 100),
            0.5,
        );
        let profile2 = SlmProfile::new(
            "p2",
            "http://localhost:2222",
            vec![2],
            TokenBudget::new(1000, 100),
            0.8,
        );

        let router = RouterEngine::new(collection, vec![profile1, profile2]);
        let stats = router.calibration_stats();
        assert_eq!(stats.len(), 2);
        assert_eq!(stats["p1"].times_selected, 0);
        assert_eq!(stats["p1"].calibrated_min_score, 0.5);
        assert_eq!(stats["p1"].original_min_score, 0.5);
        assert_eq!(stats["p2"].times_selected, 0);
        assert_eq!(stats["p2"].calibrated_min_score, 0.8);
        assert_eq!(stats["p2"].original_min_score, 0.8);
    }

    #[test]
    fn test_profile_calibration_state_recalibrate_low_confidence() {
        let mut state = ProfileCalibrationState::new(0.5);
        // Simuliere 10 Entscheidungen mit Konfidenz 0.5 (unter Schwellenwert 0.7)
        state.times_selected = 10;
        state.cumulative_confidence = 5.0; // avg = 0.5
        let adjusted = state.recalibrate(0.7);
        assert!(adjusted, "Low-confidence state muss Score erhöhen");
        assert!(
            state.calibrated_min_score > 0.5,
            "Score muss höher als original sein nach Rekalibrierung"
        );
    }

    #[test]
    fn test_profile_calibration_state_reset() {
        let mut state = ProfileCalibrationState::new(0.5);
        state.times_selected = 15;
        state.cumulative_confidence = 12.0;
        state.calibrated_min_score = 0.6;
        state.reset();
        assert_eq!(state.times_selected, 0);
        assert_eq!(state.calibrated_min_score, 0.5);
        assert_eq!(state.cumulative_confidence, 1.0);
    }

    #[tokio::test]
    async fn test_reset_calibration_per_profile() {
        let dir = tempfile::tempdir().unwrap();
        let config = memfuse_db::MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = memfuse_db::MemFuse::open_with_config(dir.path(), config)
            .await
            .unwrap();
        let collection = db.collection("default").await.unwrap();

        let profile = SlmProfile::new(
            "p1",
            "http://localhost:1111",
            vec![1],
            TokenBudget::new(1000, 100),
            0.5,
        );

        let router = RouterEngine::new(collection, vec![profile]);
        {
            let mut cal = router.calibration.write();
            if let Some(state) = cal.get_mut("p1") {
                state.times_selected = 5;
            }
        }
        assert_eq!(router.calibration_stats()["p1"].times_selected, 5);

        router.reset_calibration("p1");
        assert_eq!(router.calibration_stats()["p1"].times_selected, 0);
    }
}
