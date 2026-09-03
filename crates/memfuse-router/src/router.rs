//! Core routing engine for matching hybrid search context to SLM profiles.

use crate::profile::{ProfileCalibrationState, SlmProfile};
use memfuse_core::{ContextChunk, ContextWindow, EntityId, MemFuseError, Result};
use memfuse_db::{collection::Collection, context::ContextManager};
use memfuse_store::LsmStorage;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Calibrated confidence metrics for a routing decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceMetrics {
    /// Lower bound of the confidence interval.
    pub score_lower: f32,
    /// Upper bound of the confidence interval.
    pub score_upper: f32,
    /// Whether the score was calibrated via conformal prediction.
    pub calibrated: bool,
    /// Current conformal quantile threshold used for this decision.
    pub quantile_threshold: f32,
}

/// Result of a routing operation containing the selected profile and prepared context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// The target SLM profile selected for the query.
    pub profile: SlmProfile,
    /// The trimmed context window prepared specifically for the selected profile's token budget.
    pub context: ContextWindow,
    /// Calibrated confidence metrics for auditing and cascade control.
    pub confidence: Option<ConfidenceMetrics>,
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

        // 3. Select profile and update calibration metrics atomically within a single write lock scope
        let profile_scores = compute_profile_scores(&profiles, &chunks);

        let (_selected_profile_idx, selected_profile, confidence_metrics) = {
            let mut cal = self.calibration.write();
            let (selected_idx, profile, metrics) =
                self.select_profile_cascade(&chunks, &profiles, &cal)?;

            let best_score = profile_scores.get(&selected_idx).copied().unwrap_or(0.0);
            let second_best = profile_scores
                .iter()
                .filter(|(idx, _)| **idx != selected_idx)
                .map(|(_, s)| *s)
                .fold(0.0f32, f32::max);
            let confidence = if second_best > 0.0 {
                (best_score / second_best) as f64
            } else {
                2.0 // Nur ein Kandidat → hohe Konfidenz
            };

            if let Some(state) = cal.get_mut(&profile.name) {
                state.times_selected += 1;
                state.cumulative_confidence += confidence;

                // Non-conformity score: inverse of confidence ratio, clamped to [0, 1]
                let non_conformity = if confidence > 0.0 {
                    (1.0 / confidence as f32).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                state.recalibrate_conformal(non_conformity);
            }

            (selected_idx, profile, metrics)
        };

        // 4. Construct ContextWindow using ContextManager tailored to selected_profile.token_budget
        let raw_chunks: Vec<ContextChunk> = chunks.into_iter().map(|(c, _)| c).collect();
        let mut context_mgr = ContextManager::new(selected_profile.token_budget.clone());
        context_mgr.set_relevance_threshold(0.0);
        let context_window = context_mgr.prepare_context(raw_chunks)?;

        Ok(RoutingDecision {
            profile: selected_profile,
            context: context_window,
            confidence: Some(confidence_metrics),
        })
    }

    /// Kalibriertes Kaskaden-Routing.
    ///
    /// Algorithmus:
    /// 1. Sortiere Profile absteigend nach min_relevance_score (präzisestes zuerst).
    /// 2. Für jedes Profil in dieser Reihenfolge:
    ///    - Berechne Aggregat-Score der Chunks (existing logic)
    ///    - Hole ConformalCalibrator für dieses Profil aus self.calibration
    ///    - Prüfe: score >= calibrator.quantile_threshold (oder profile.min_relevance_score)
    ///      JA: Dieses Profil nehmen, ConfidenceMetrics.calibrated = true
    ///      NEIN: Weiter zum nächsten Profil (Kaskade)
    /// 3. Falls kein Profil den kalibrierten Schwellenwert erfüllt:
    ///    - Nehme das letzte (geringstes min_relevance_score) als sicheren Fallback
    ///    - ConfidenceMetrics.calibrated = false, tracing::warn! ausgeben
    ///
    /// # Returns
    /// (profil_index, SlmProfile, ConfidenceMetrics)
    pub(crate) fn select_profile_cascade(
        &self,
        chunks: &[(ContextChunk, Option<u64>)],
        profiles: &[SlmProfile],
        calibration: &HashMap<String, ProfileCalibrationState>,
    ) -> Result<(usize, SlmProfile, ConfidenceMetrics)> {
        if chunks.is_empty() {
            return Err(MemFuseError::NotFound(
                "Keine gültigen Chunks aus Suchergebnissen ermittelbar".to_string(),
            ));
        }

        if !chunks.iter().any(|(c, _)| c.relevance.is_finite()) {
            tracing::error!(
                "Alle Chunk-Relevanzwerte sind NaN/Inf — mögliche Upstream-Korruption in der Distanzberechnung"
            );
            return Err(MemFuseError::NotFound(
                "Alle Chunk-Relevanzwerte sind NaN/Inf — mögliche Upstream-Korruption in der Distanzberechnung".to_string(),
            ));
        }

        if profiles.is_empty() {
            return Err(MemFuseError::NotFound(
                "Keine SLM-Profile konfiguriert".to_string(),
            ));
        }

        // Filter profiles by community match eligibility.
        // A profile is eligible if its domain_communities is empty, OR if at least one chunk matches one of its domain_communities.
        let eligible_profiles: Vec<(usize, &SlmProfile)> = profiles
            .iter()
            .enumerate()
            .filter(|(_, profile)| {
                profile.domain_communities.is_empty()
                    || chunks.iter().any(|(_, comm_id)| {
                        comm_id.is_some_and(|cid| profile.domain_communities.contains(&cid))
                    })
            })
            .collect();

        if eligible_profiles.is_empty() {
            return Err(MemFuseError::NotFound(
                "Kein SLM-Profil entspricht der Community-Zuordnung".to_string(),
            ));
        }

        // 1. Sort eligible profile indices descending by min_relevance_score (most precise first).
        // Tie-breaking: when min_relevance_scores are equal, candidate score descending, then lower original index.
        let mut sorted_profiles = eligible_profiles;
        sorted_profiles.sort_by(|(idx_a, a), (idx_b, b)| {
            b.min_relevance_score
                .total_cmp(&a.min_relevance_score)
                .then_with(|| {
                    let score_a = compute_profile_score(a, chunks);
                    let score_b = compute_profile_score(b, chunks);
                    score_b.total_cmp(&score_a).then_with(|| idx_a.cmp(idx_b))
                })
        });

        // 2. Cascade evaluation in descending min_relevance_score order
        for &(orig_idx, profile) in &sorted_profiles {
            let score = compute_profile_score(profile, chunks);
            let state = calibration.get(&profile.name);

            let (threshold, is_calibrated) = match state {
                Some(st) if st.conformal.window_total > 10 => (st.calibrated_min_score, true),
                _ => (profile.min_relevance_score, false),
            };

            if score >= threshold {
                let q_threshold = state
                    .map(|st| st.conformal.quantile_threshold)
                    .unwrap_or(profile.min_relevance_score);
                let confidence = ConfidenceMetrics {
                    score_lower: score * 0.9,
                    score_upper: score * 1.1,
                    calibrated: is_calibrated,
                    quantile_threshold: q_threshold,
                };
                return Ok((orig_idx, profile.clone(), confidence));
            }
        }

        // 3. Fallback: Take the eligible profile with lowest min_relevance_score (last in cascade)
        let &(fallback_idx, fallback_profile) = match sorted_profiles.last() {
            Some(p) => p,
            None => {
                return Err(MemFuseError::NotFound(
                    "Keine SLM-Profile konfiguriert".to_string(),
                ));
            }
        };
        let fallback_score = compute_profile_score(fallback_profile, chunks);
        let state = calibration.get(&fallback_profile.name);
        let q_threshold = state
            .map(|st| st.conformal.quantile_threshold)
            .unwrap_or(fallback_profile.min_relevance_score);

        tracing::warn!(
            profile = %fallback_profile.name,
            "Kaskaden-Fallback: Kein Profil über Schwellenwert, nutze Profil mit niedrigstem min_relevance_score"
        );

        let confidence = ConfidenceMetrics {
            score_lower: fallback_score * 0.9,
            score_upper: fallback_score * 1.1,
            calibrated: false,
            quantile_threshold: q_threshold,
        };

        Ok((fallback_idx, fallback_profile.clone(), confidence))
    }
}

/// Computes the aggregate relevance score for a single profile across chunks.
pub(crate) fn compute_profile_score(
    profile: &SlmProfile,
    chunks: &[(ContextChunk, Option<u64>)],
) -> f32 {
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
    aggregated_score
}

/// Computes candidate scores for all profiles across chunks.
pub(crate) fn compute_profile_scores(
    profiles: &[SlmProfile],
    chunks: &[(ContextChunk, Option<u64>)],
) -> HashMap<usize, f32> {
    profiles
        .iter()
        .enumerate()
        .map(|(idx, profile)| (idx, compute_profile_score(profile, chunks)))
        .collect()
}

#[allow(dead_code)]
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

#[allow(dead_code)]
pub(crate) fn select_profile_from_chunks(
    profiles: &[SlmProfile],
    chunks: &[(ContextChunk, Option<u64>)],
) -> Result<usize> {
    if chunks.is_empty() {
        return Err(MemFuseError::NotFound(
            "Keine gültigen Chunks aus Suchergebnissen ermittelbar".to_string(),
        ));
    }

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
