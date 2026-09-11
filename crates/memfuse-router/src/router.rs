// FILE-CONTEXT
// STAND: 2026-09-10T19:16:25Z (SESSION: 3f3e4637)
// ZWECK: Haupt-Routing-Engine für Hybrid-Search-Kontext auf SLM-Profile.
// INVARIANTEN: Atomare Snapshot-Sicherheit bei Hot-Reload, NaN-Safety bei Distanz-Eingaben.
// NICHT-OFFENSICHTLICH: EntityId::from_doc_id Vermeidung von String-Rehashing; Bounded Pending Map.
// SIEHE AUCH: docs/decisions/ADR-020-memfuse-brain.md, rules/tag_taxonomy.md

//! Core routing engine for matching hybrid search context to SLM profiles.

use crate::lyapunov::{LyapunovDriftWatcher, LyapunovResult};
use crate::outcome::{DecisionId, RoutingOutcome};
use crate::profile::{ProfileCalibrationState, SlmProfile};
use arc_swap::ArcSwap;
use memfuse_core::{ContextChunk, ContextWindow, EntityId, MemFuseError, Result};
use memfuse_db::{collection::Collection, context::ContextManager};
use memfuse_store::LsmStorage;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Minimum calibration samples before conformal quantiles are considered reliable.
/// Statistical basis: ≥100 samples required for α=0.1 coverage guarantee per
/// Venn & Gammerman (2005). At 30 samples the quantile interval is too wide
/// to provide meaningful routing signal — the router behaves as a random router.
pub(crate) const CALIBRATION_WARMUP_WINDOW: u32 = 100;

/// Minimum calibration samples required for high confidence coverage (α=0.05).
#[allow(dead_code)]
pub(crate) const CALIBRATION_HIGH_CONFIDENCE_WINDOW: u32 = 200;

/// Maximale TTL für ausstehende Routing-Entscheidungen bevor sie bereinigt werden.
pub(crate) const PENDING_DECISION_TTL: Duration = Duration::from_secs(300);

/// Maximale Kapazität der Map ausstehender Routing-Entscheidungen.
pub(crate) const MAX_PENDING_DECISIONS: usize = 10_000;

/// Calibrated confidence metrics for a routing decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfidenceMetrics {
    /// Lower bound of the confidence interval (None when not calibrated).
    pub score_lower: Option<f32>,
    /// Upper bound of the confidence interval (None when not calibrated).
    pub score_upper: Option<f32>,
    /// Whether the score was calibrated via outcome-driven conformal calibration.
    pub calibrated: bool,
    /// Current conformal quantile threshold used for this decision.
    pub quantile_threshold: f32,
    /// Non-conformity score of the decision.
    pub non_conformity_score: f32,
    /// Margin/ratio between best and second best score.
    pub selection_margin: f32,
}

/// Result of a routing operation containing the selected profile, prepared context, confidence, and decision ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// The target SLM profile selected for the query.
    pub profile: SlmProfile,
    /// The trimmed context window prepared specifically for the selected profile's token budget.
    pub context: ContextWindow,
    /// Calibrated confidence metrics for auditing and cascade control.
    pub confidence: Option<ConfidenceMetrics>,
    /// Eindeutige ID dieser Routing-Entscheidung.
    pub decision_id: DecisionId,
    /// Lyapunov-Drift-Status zum Zeitpunkt der Entscheidung (None wenn InsufficientData).
    pub drift_status: Option<LyapunovResult>,
}

/// Inner state for `RouterEngine` holding active profiles, calibration states, and Lyapunov drift watchers.
///
/// Atomic state swap via ArcSwap: profiles, calibration, and watchers
/// are always seen as a consistent unit by all readers.
#[derive(Clone)]
pub struct RouterState {
    pub profiles: Vec<SlmProfile>,
    pub calibration: HashMap<String, ProfileCalibrationState>,
    pub lyapunov_watchers: HashMap<String, LyapunovDriftWatcher>,
}

/// Router engine that routes queries to optimal SLM backends based on community assignment and search scores.
pub struct RouterEngine {
    collection: Arc<Collection<LsmStorage>>,
    /// Atomic state swap via ArcSwap: profiles, calibration, and watchers
    /// are always seen as a consistent unit by all readers.
    pub(crate) state: ArcSwap<RouterState>,
    /// Kept in a separate RwLock to avoid cloning overhead on high-frequency routing decision tracking writes.
    pub(crate) pending_decisions: RwLock<HashMap<DecisionId, (String, Instant)>>,
}

impl RouterEngine {
    /// Creates a new `RouterEngine` instance.
    pub fn new(
        collection: Arc<Collection<LsmStorage>>,
        profiles: Vec<SlmProfile>,
        calibration_store_path: Option<std::path::PathBuf>,
    ) -> Self {
        let mut calibration: HashMap<String, ProfileCalibrationState> = profiles
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    ProfileCalibrationState::new(p.min_relevance_score),
                )
            })
            .collect();

        let lyapunov_watchers: HashMap<String, LyapunovDriftWatcher> = profiles
            .iter()
            .map(|p| (p.name.clone(), LyapunovDriftWatcher::default()))
            .collect();

        if let Some(ref path) = calibration_store_path {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(persisted) =
                    serde_json::from_slice::<HashMap<String, ProfileCalibrationState>>(&bytes)
                {
                    // Merge persisted state into defaults (persisted wins for known profiles)
                    for (name, state) in persisted {
                        if calibration.contains_key(&name) {
                            calibration.insert(name, state);
                        }
                        // Unknown profiles (removed from config) are silently dropped
                    }
                }
            }
        }

        let router_state = RouterState {
            profiles,
            calibration,
            lyapunov_watchers,
        };

        Self {
            collection,
            state: ArcSwap::from(Arc::new(router_state)),
            pending_decisions: RwLock::new(HashMap::new()),
        }
    }

    /// Validates all profiles and creates a new `RouterEngine` instance.
    pub fn try_new(
        collection: Arc<Collection<LsmStorage>>,
        profiles: Vec<SlmProfile>,
        calibration_store_path: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        for p in &profiles {
            p.validate()?;
        }
        Ok(Self::new(collection, profiles, calibration_store_path))
    }

    /// Dynamically updates configured SLM profiles at runtime (Hot-Reload).
    pub fn update_profiles(&self, new_profiles: Vec<SlmProfile>) {
        let current = self.state.load_full();
        let mut old_cal = current.calibration.clone();
        let new_cal: HashMap<String, ProfileCalibrationState> = new_profiles
            .iter()
            .map(|p| {
                let mut state = old_cal
                    .remove(&p.name)
                    .unwrap_or_else(|| ProfileCalibrationState::new(p.min_relevance_score));
                state.check_and_invalidate_fingerprint(p.fingerprint.as_ref());
                (p.name.clone(), state)
            })
            .collect();

        let mut old_watchers = current.lyapunov_watchers.clone();
        let new_watchers: HashMap<String, LyapunovDriftWatcher> = new_profiles
            .iter()
            .map(|p| {
                let watcher = old_watchers.remove(&p.name).unwrap_or_default();
                (p.name.clone(), watcher)
            })
            .collect();

        let new_state = RouterState {
            profiles: new_profiles,
            calibration: new_cal,
            lyapunov_watchers: new_watchers,
        };

        self.state.store(Arc::new(new_state));
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
        self.state.load().profiles.clone()
    }

    /// Gibt aktuelle Kalibrierungsstatistik für alle Profile zurück.
    pub fn calibration_stats(&self) -> HashMap<String, ProfileCalibrationState> {
        self.state.load().calibration.clone()
    }

    /// Setzt Kalibrierungsstatistik für ein bestimmtes Profil zurück.
    pub fn reset_calibration(&self, profile_name: &str) {
        let current = self.state.load_full();
        if current.calibration.contains_key(profile_name) {
            let mut new_state = (*current).clone();
            if let Some(state) = new_state.calibration.get_mut(profile_name) {
                state.reset();
            }
            self.state.store(Arc::new(new_state));
        }
    }

    /// Gibt den aktuellen Lyapunov-Drift-Status für ein Profil zurück.
    pub fn drift_status(&self, profile_name: &str) -> Option<LyapunovResult> {
        self.state
            .load()
            .lyapunov_watchers
            .get(profile_name)
            .and_then(|w| w.latest_result.clone())
    }

    /// Setzt die Baseline für den Lyapunov-Drift-Wächter eines bestimmten Profils.
    pub fn set_lyapunov_baseline(&self, profile_name: &str, baseline: &[f32]) -> bool {
        let current = self.state.load_full();
        if current.lyapunov_watchers.contains_key(profile_name) {
            let mut new_state = (*current).clone();
            if let Some(watcher) = new_state.lyapunov_watchers.get_mut(profile_name) {
                watcher.set_baseline(baseline);
            }
            self.state.store(Arc::new(new_state));
            true
        } else {
            false
        }
    }

    fn evict_stale_decisions(&self) {
        let now = Instant::now();
        let cutoff = now.checked_sub(PENDING_DECISION_TTL);
        let mut map = self.pending_decisions.write();
        if map.len() >= MAX_PENDING_DECISIONS {
            if let Some(cutoff) = cutoff {
                map.retain(|_, (_, ts)| *ts > cutoff);
            }
        }
    }

    /// Muss vom Aufrufer (Agent-Loop) nach Abschluss des SLM-Aufrufs aufgerufen werden.
    /// Liefert das tatsächliche Ergebnis zurück und trainiert die Kalibrierung
    /// mit einem echten Ground-Truth-Signal.
    ///
    /// Gibt true zurück wenn die Decision gefunden und verarbeitet wurde,
    /// false wenn die DecisionId unbekannt ist (z.B. nach Restart).
    pub fn record_outcome(&self, decision_id: DecisionId, outcome: RoutingOutcome) -> bool {
        let profile_name = match self.pending_decisions.write().remove(&decision_id) {
            Some((name, _ts)) => name,
            None => {
                tracing::warn!(
                    ?decision_id,
                    "record_outcome: unbekannte DecisionId ignoriert"
                );
                return false;
            }
        };

        let current = self.state.load_full();
        let active_fp = current
            .profiles
            .iter()
            .find(|p| p.name == profile_name)
            .and_then(|p| p.fingerprint.clone());

        let non_conformity = outcome.non_conformity_score();

        let mut new_state = (*current).clone();
        if let Some(state) = new_state.calibration.get_mut(&profile_name) {
            state.check_and_invalidate_fingerprint(active_fp.as_ref());
            if active_fp.is_some() {
                state.recalibrate_conformal(non_conformity);
                tracing::debug!(
                    profile = %profile_name,
                    ?outcome,
                    non_conformity,
                    "Router outcome recorded"
                );
            }
        }
        self.state.store(Arc::new(new_state));
        true
    }

    /// Anzahl offener (noch nicht mit record_outcome() abgeschlossener) Decisions.
    /// Sollte in normaler Laufzeit nahe 0 bleiben.
    pub fn pending_decision_count(&self) -> usize {
        self.pending_decisions.read().len()
    }

    /// Setzt Kalibrierungsstatistik für alle Profile zurück.
    pub fn reset_all_calibration(&self) {
        let current = self.state.load_full();
        let mut new_state = (*current).clone();
        for state in new_state.calibration.values_mut() {
            state.reset();
        }
        self.state.store(Arc::new(new_state));
    }

    /// Routes a query with embedding and text to the best matching SLM profile.
    #[allow(deprecated)]
    pub async fn route(
        &self,
        query_embedding: &[f32],
        query_text: &str,
    ) -> Result<RoutingDecision> {
        self.evict_stale_decisions();

        if query_embedding.iter().any(|v| !v.is_finite()) {
            return Err(MemFuseError::InvalidInput(
                "query_embedding contains non-finite values (NaN/Inf)".to_string(),
            ));
        }

        // Snapshot state atomically via ArcSwap to guarantee caller consistency during hot-reloads
        let state_snap = self.state.load_full();
        let profiles = state_snap.profiles.clone();

        if profiles.is_empty() {
            return Err(MemFuseError::NotFound(
                "Keine SLM-Profile für Routing konfiguriert".to_string(),
            ));
        }

        // 1. Perform hybrid search with standard fusion weights
        let search_results = self
            .collection
            .query()
            .text(query_text)
            .embedding(query_embedding)
            .k(10)
            .execute()
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

        // 3. Perform profile selection, scoring, calibration tracking, and confidence metric generation
        // using an updated state snapshot swapped atomically via ArcSwap.
        let current_state = self.state.load_full();
        let mut new_state = (*current_state).clone();

        let (selected_profile, confidence_metrics) = {
            let cal = &mut new_state.calibration;

            // 1. Derive effective profiles using calibrated_min_score from calibration state
            let effective_profiles: Vec<SlmProfile> = profiles
                .iter()
                .map(|p| {
                    let mut ep = p.clone();
                    if let Some(state) = cal.get_mut(&p.name) {
                        state.check_and_invalidate_fingerprint(p.fingerprint.as_ref());
                        if state.is_calibrated(p.fingerprint.as_ref()) {
                            ep.min_relevance_score = state.calibrated_min_score;
                        }
                    }
                    ep
                })
                .collect();

            // 2. Scoring + Cascade Selection
            let (selected_idx, selected_profile, _) =
                self.select_profile_cascade(&chunks, &effective_profiles, cal)?;

            let profile_scores = compute_profile_scores(&profiles, &chunks);
            let best_score = profile_scores.get(&selected_idx).copied().unwrap_or(0.0);
            let second_best = profile_scores
                .iter()
                .filter(|(idx, _)| **idx != selected_idx)
                .map(|(_, s)| *s)
                .fold(0.0f32, f32::max);

            let confidence_ratio = if second_best > 0.0 {
                (best_score / second_best) as f64
            } else {
                2.0 // Single candidate -> high confidence
            };

            let non_conformity = if best_score > 0.0 {
                (1.0 / confidence_ratio as f32).clamp(0.0, 1.0)
            } else {
                1.0
            };

            if let Some(state) = cal.get_mut(&selected_profile.name) {
                state.times_selected += 1;
                state.cumulative_confidence += confidence_ratio;
            }

            // 4. Construct ConfidenceMetrics from updated state
            let metrics = cal.get(&selected_profile.name).map(|state| {
                let calibrated = state.conformal.window_total >= CALIBRATION_WARMUP_WINDOW as u64;
                if !calibrated {
                    tracing::info!(
                        profile = %selected_profile.name,
                        samples = state.conformal.window_total,
                        required = CALIBRATION_WARMUP_WINDOW,
                        "Router in random-fallback mode: calibration not yet reliable"
                    );
                }
                ConfidenceMetrics {
                    score_lower: if calibrated {
                        Some(best_score * (1.0 - state.conformal.alpha))
                    } else {
                        None
                    },
                    score_upper: if calibrated {
                        Some(best_score * (1.0 + state.conformal.alpha))
                    } else {
                        None
                    },
                    calibrated,
                    quantile_threshold: state.conformal.quantile_threshold,
                    non_conformity_score: non_conformity,
                    selection_margin: confidence_ratio as f32,
                }
            });

            (selected_profile, metrics)
        };

        let decision_id = DecisionId::new();
        self.pending_decisions
            .write()
            .insert(decision_id, (selected_profile.name.clone(), Instant::now()));

        // 4. Construct ContextWindow using ContextManager tailored to selected_profile.token_budget and min_relevance_score
        let raw_chunks: Vec<ContextChunk> = chunks.into_iter().map(|(c, _)| c).collect();

        // 5. Update Lyapunov Drift Watcher with non-conformity score
        let non_conformity_score = confidence_metrics
            .as_ref()
            .map(|m| m.non_conformity_score)
            .unwrap_or(1.0);

        let drift_status = {
            let watchers = &mut new_state.lyapunov_watchers;
            if let Some(watcher) = watchers.get_mut(&selected_profile.name) {
                watcher.observe_score(non_conformity_score);
                let res = watcher.analyze();

                if let LyapunovResult::DriftDetected {
                    lyapunov_exponent,
                    ref reason,
                } = res
                {
                    tracing::warn!(
                        profile = %selected_profile.name,
                        lambda = lyapunov_exponent,
                        kl_divergence = reason.kl_divergence,
                        "Lyapunov drift detected — conformal calibration may be stale"
                    );
                }

                match res {
                    LyapunovResult::InsufficientData => None,
                    status => Some(status),
                }
            } else {
                None
            }
        };

        // Store updated state atomically via ArcSwap
        self.state.store(Arc::new(new_state));

        let mut context_mgr = ContextManager::new(selected_profile.token_budget.clone());
        context_mgr.set_relevance_threshold(selected_profile.min_relevance_score);
        let context_window = context_mgr.prepare_context(raw_chunks)?;

        Ok(RoutingDecision {
            profile: selected_profile,
            context: context_window,
            confidence: confidence_metrics,
            decision_id,
            drift_status,
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
    ///      JA: Dieses Profil nehmen, ConfidenceMetrics::Calibrated
    ///      NEIN: Weiter zum nächsten Profil (Kaskade)
    /// 3. Falls kein Profil den kalibrierten Schwellenwert erfüllt:
    ///    - Nehme das letzte (geringstes min_relevance_score) als sicheren Fallback
    ///    - ConfidenceMetrics::Uncalibrated, tracing::warn! ausgeben
    ///
    /// # Returns
    /// (profil_index, SlmProfile, ConfidenceMetrics)
    pub(crate) fn select_profile_cascade(
        &self,
        chunks: &[(ContextChunk, Option<u64>)],
        profiles: &[SlmProfile],
        calibration: &mut HashMap<String, ProfileCalibrationState>,
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
        let mut sorted_profiles = eligible_profiles.clone();
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
            let state = calibration.get_mut(&profile.name);

            let (threshold, is_calibrated) = match state {
                Some(st) => {
                    st.check_and_invalidate_fingerprint(profile.fingerprint.as_ref());
                    if st.is_calibrated(profile.fingerprint.as_ref()) {
                        (st.calibrated_min_score, true)
                    } else {
                        (profile.min_relevance_score, false)
                    }
                }
                None => (profile.min_relevance_score, false),
            };

            if score >= threshold {
                let (quantile, alpha) = match calibration.get(&profile.name) {
                    Some(st) => (st.conformal.quantile_threshold, st.conformal.alpha),
                    None => (profile.min_relevance_score, 0.05),
                };
                let non_conformity = (1.0 - (score / quantile.max(f32::EPSILON))).clamp(0.0, 1.0);
                let selection_margin = if quantile > 0.0 {
                    score / quantile
                } else {
                    1.0
                };

                let confidence = ConfidenceMetrics {
                    score_lower: if is_calibrated {
                        Some(score * (1.0 - alpha))
                    } else {
                        None
                    },
                    score_upper: if is_calibrated {
                        Some(score * (1.0 + alpha))
                    } else {
                        None
                    },
                    calibrated: is_calibrated,
                    quantile_threshold: quantile,
                    non_conformity_score: non_conformity,
                    selection_margin,
                };
                return Ok((orig_idx, profile.clone(), confidence));
            }
        }

        // Während der Warmup-Periode (calibrated == false) wird bewusst konservativ geroutet:
        // das ressourcenschonendste Profil wird gewählt, um Kostenrisiken bei fehlender
        // statistischer Absicherung zu minimieren.
        let &(fallback_idx, fallback_profile) =
            match eligible_profiles.iter().min_by(|(idx_a, a), (idx_b, b)| {
                a.estimated_cost()
                    .total_cmp(&b.estimated_cost())
                    .then_with(|| a.min_relevance_score.total_cmp(&b.min_relevance_score))
                    .then_with(|| idx_a.cmp(idx_b))
            }) {
                Some(p) => p,
                None => {
                    return Err(MemFuseError::NotFound(
                        "Keine SLM-Profile konfiguriert".to_string(),
                    ));
                }
            };
        let fallback_score = compute_profile_score(fallback_profile, chunks);
        let state = calibration.get(&fallback_profile.name);
        let (q_threshold, alpha, is_calibrated) = match state {
            Some(st) => (
                st.conformal.quantile_threshold,
                st.conformal.alpha,
                st.is_calibrated(fallback_profile.fingerprint.as_ref()),
            ),
            None => (fallback_profile.min_relevance_score, 0.05, false),
        };
        let non_conformity =
            (1.0 - (fallback_score / q_threshold.max(f32::EPSILON))).clamp(0.0, 1.0);
        let selection_margin = if q_threshold > 0.0 {
            fallback_score / q_threshold
        } else {
            1.0
        };

        tracing::warn!(
            profile = %fallback_profile.name,
            "Kaskaden-Fallback: Kein Profil über Schwellenwert, nutze Profil mit niedrigstem min_relevance_score"
        );

        let confidence = ConfidenceMetrics {
            score_lower: if is_calibrated {
                Some(fallback_score * (1.0 - alpha))
            } else {
                None
            },
            score_upper: if is_calibrated {
                Some(fallback_score * (1.0 + alpha))
            } else {
                None
            },
            calibrated: is_calibrated,
            quantile_threshold: q_threshold,
            non_conformity_score: non_conformity,
            selection_margin,
        };

        Ok((fallback_idx, fallback_profile.clone(), confidence))
    }
}

pub(crate) const COMMUNITY_RELEVANCE_BOOST: f32 = 1.2;

#[derive(Debug, Clone)]
pub(crate) struct ProfileScoring {
    pub aggregated_score: f32,
    pub max_score: f32,
    pub community_matched: bool,
}

pub(crate) fn score_profile(
    profile: &SlmProfile,
    chunks: &[(ContextChunk, Option<u64>)],
) -> ProfileScoring {
    let mut aggregated_score = 0.0f32;
    let mut max_score = 0.0f32;
    let mut community_matched = profile.domain_communities.is_empty();

    for (chunk, comm_id) in chunks {
        if !chunk.relevance.is_finite() {
            continue;
        }

        let is_match = profile.domain_communities.is_empty()
            || comm_id.is_some_and(|cid| profile.domain_communities.contains(&cid));

        if is_match {
            community_matched = true;
            let boost = if comm_id.is_some_and(|cid| profile.domain_communities.contains(&cid)) {
                COMMUNITY_RELEVANCE_BOOST
            } else {
                1.0
            };
            let boosted_relevance = chunk.relevance * boost;
            aggregated_score += boosted_relevance;
            if boosted_relevance > max_score {
                max_score = boosted_relevance;
            }
        }
    }

    ProfileScoring {
        aggregated_score,
        max_score,
        community_matched,
    }
}

/// Computes the aggregate relevance score for a single profile across chunks.
pub(crate) fn compute_profile_score(
    profile: &SlmProfile,
    chunks: &[(ContextChunk, Option<u64>)],
) -> f32 {
    score_profile(profile, chunks).aggregated_score
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
    score_profile(profile, chunks).max_score
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
        let scoring = score_profile(profile, chunks);
        if scoring.community_matched {
            any_community_matched = true;
        }

        if scoring.community_matched && scoring.aggregated_score >= profile.min_relevance_score {
            profile_scores.insert(idx, scoring.aggregated_score);
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
    async fn test_calibration_stats_initial_state(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let config = memfuse_db::MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = memfuse_db::MemFuse::open_with_config(dir.path(), config).await?;
        let collection = db.collection("default").await?;

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

        let router = RouterEngine::new(collection, vec![profile1, profile2], None);
        let stats = router.calibration_stats();
        assert_eq!(stats.len(), 2);
        assert_eq!(stats["p1"].times_selected, 0);
        assert_eq!(stats["p1"].calibrated_min_score, 0.5);
        assert_eq!(stats["p1"].original_min_score, 0.5);
        assert_eq!(stats["p2"].times_selected, 0);
        assert_eq!(stats["p2"].calibrated_min_score, 0.8);
        assert_eq!(stats["p2"].original_min_score, 0.8);
        Ok(())
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
    async fn test_reset_calibration_per_profile(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let config = memfuse_db::MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = memfuse_db::MemFuse::open_with_config(dir.path(), config).await?;
        let collection = db.collection("default").await?;

        let profile = SlmProfile::new(
            "p1",
            "http://localhost:1111",
            vec![1],
            TokenBudget::new(1000, 100),
            0.5,
        );

        let router = RouterEngine::new(collection, vec![profile], None);
        {
            let current = router.state.load_full();
            let mut new_state = (*current).clone();
            if let Some(state) = new_state.calibration.get_mut("p1") {
                state.times_selected = 5;
            }
            router.state.store(Arc::new(new_state));
        }
        assert_eq!(router.calibration_stats()["p1"].times_selected, 5);

        router.reset_calibration("p1");
        assert_eq!(router.calibration_stats()["p1"].times_selected, 0);
        Ok(())
    }
}
