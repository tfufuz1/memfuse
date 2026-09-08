// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Parameter-Sweep für PathRAG sufficiency_threshold über LongMemEval und LoCoMo Datensätze.
// INVARIANTEN: Misst Precision und Recall für Threshold-Werte ∈ {0.01, 0.1, 0.3, 0.6}.

use crate::locomo::{load_locomo_dataset, LocomoQuestionCategory};
use crate::long_mem_eval::{LongMemEvalQuestionType, RegressionSuite};
use memfuse_core::Result;
use memfuse_db::{MemFuse, MemFuseConfig, SearchStrategy};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tempfile::TempDir;

fn pad_vector(v: &[f32], target_dim: usize) -> Vec<f32> {
    let mut padded = vec![0.0f32; target_dim];
    for (i, &val) in v.iter().enumerate().take(target_dim) {
        padded[i] = val;
    }
    padded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRagSweepMetric {
    pub threshold: f64,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub precision_at_5: f64,
    pub precision_at_10: f64,
    pub total_queries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRagSweepReport {
    pub timestamp: String,
    pub long_mem_eval_sweep: Vec<PathRagSweepMetric>,
    pub locomo_sweep: Vec<PathRagSweepMetric>,
}

pub async fn run_pathrag_sweep_long_mem_eval(
    thresholds: &[f64],
) -> Result<Vec<PathRagSweepMetric>> {
    const DIM: usize = 768;
    let suite = RegressionSuite::baseline();
    let db_cfg = MemFuseConfig {
        dimension: DIM,
        ..Default::default()
    };

    let temp_dir = TempDir::new()?;
    let db = MemFuse::open_with_config(temp_dir.path(), db_cfg).await?;
    let col = db.collection("lme_pathrag_sweep").await?;

    let dummy_vec = pad_vector(&[0.5, 0.5, 0.0, 0.0], DIM);

    for scenario in &suite.scenarios {
        for session in &scenario.sessions {
            let mut prev_doc: Option<String> = None;
            for turn in &session.turns {
                let metadata = serde_json::json!({
                    "text": turn.text,
                    "speaker": turn.speaker,
                    "session_id": session.session_id,
                    "scenario_id": scenario.scenario_id,
                });
                col.insert(&turn.doc_id, &dummy_vec, Some(metadata)).await?;
                if let Some(ref prev) = prev_doc {
                    col.relate(prev, &turn.doc_id, "session_turn_flow").await?;
                }
                prev_doc = Some(turn.doc_id.clone());
            }
        }
    }

    let mut metrics = Vec::new();

    for &t in thresholds {
        let mut rec5_hits = 0;
        let mut rec10_hits = 0;
        let mut prec5_sum = 0.0;
        let mut prec10_sum = 0.0;

        for scenario in &suite.scenarios {
            let res = col
                .query()
                .text(&scenario.query)
                .strategy(SearchStrategy::PathRag {
                    max_hops: 4,
                    sufficiency_threshold: t,
                })
                .k(10)
                .execute()
                .await?;

            if scenario.question_type == LongMemEvalQuestionType::Abstention {
                let is_hit = res.is_empty()
                    || res.iter().all(|r| {
                        r.score < 0.1 || (!r.id.contains("nuclear") && !r.id.contains("Mars"))
                    });
                if is_hit {
                    rec5_hits += 1;
                    rec10_hits += 1;
                    prec5_sum += 1.0;
                    prec10_sum += 1.0;
                }
            } else {
                let ret_5: Vec<_> = res.iter().take(5).collect();
                let ret_10: Vec<_> = res.iter().take(10).collect();

                let is_relevant = |doc_id: &str, metadata: Option<&serde_json::Value>| {
                    if doc_id == scenario.expected_answer_doc_id {
                        return true;
                    }
                    if let Some(m) = metadata {
                        if let Some(txt) = m.get("text").and_then(|v| v.as_str()) {
                            let lower = txt.to_lowercase();
                            if !scenario.expected_keywords.is_empty()
                                && scenario
                                    .expected_keywords
                                    .iter()
                                    .all(|kw| lower.contains(&kw.to_lowercase()))
                            {
                                return true;
                            }
                        }
                    }
                    false
                };

                let match_count_5 = ret_5
                    .iter()
                    .filter(|r| is_relevant(&r.id, r.metadata.as_ref()))
                    .count();
                let match_count_10 = ret_10
                    .iter()
                    .filter(|r| is_relevant(&r.id, r.metadata.as_ref()))
                    .count();

                if match_count_5 > 0 {
                    rec5_hits += 1;
                }
                if match_count_10 > 0 {
                    rec10_hits += 1;
                }

                let prec_5 = if !ret_5.is_empty() {
                    match_count_5 as f64 / ret_5.len() as f64
                } else {
                    0.0
                };

                let prec_10 = if !ret_10.is_empty() {
                    match_count_10 as f64 / ret_10.len() as f64
                } else {
                    0.0
                };

                prec5_sum += prec_5;
                prec10_sum += prec_10;
            }
        }

        let total = suite.scenarios.len() as f64;
        metrics.push(PathRagSweepMetric {
            threshold: t,
            recall_at_5: rec5_hits as f64 / total,
            recall_at_10: rec10_hits as f64 / total,
            precision_at_5: prec5_sum / total,
            precision_at_10: prec10_sum / total,
            total_queries: suite.scenarios.len(),
        });
    }

    Ok(metrics)
}

pub async fn run_pathrag_sweep_locomo(
    locomo_dataset_path: &Path,
    thresholds: &[f64],
) -> Result<Vec<PathRagSweepMetric>> {
    const DIM: usize = 768;
    let cases = load_locomo_dataset(locomo_dataset_path)?;
    let eval_cases: Vec<_> = cases
        .iter()
        .filter(|c| c.category != LocomoQuestionCategory::Adversarial)
        .collect();

    let db_cfg = MemFuseConfig {
        dimension: DIM,
        ..Default::default()
    };

    let temp_dir = TempDir::new()?;
    let db = MemFuse::open_with_config(temp_dir.path(), db_cfg).await?;
    let col = db.collection("locomo_pathrag_sweep").await?;

    let dummy_vec = pad_vector(&[0.5, 0.5, 0.0, 0.0], DIM);

    for (case_idx, case) in eval_cases.iter().enumerate() {
        let mut prev_doc: Option<String> = None;
        for (ev_idx, ev) in case.evidence.iter().enumerate() {
            let doc_id = format!("locomo_doc_{}_{}", case_idx, ev_idx);
            let metadata = serde_json::json!({
                "text": ev,
                "case_id": case.question_id,
                "sample_id": case.sample_id,
            });
            col.insert(&doc_id, &dummy_vec, Some(metadata)).await?;
            if let Some(ref prev) = prev_doc {
                col.relate(prev, &doc_id, "evidence_flow").await?;
            }
            prev_doc = Some(doc_id);
        }
    }

    let mut metrics = Vec::new();

    for &t in thresholds {
        let mut rec5_hits = 0;
        let mut rec10_hits = 0;
        let mut prec5_sum = 0.0;
        let mut prec10_sum = 0.0;

        for case in &eval_cases {
            let res = col
                .query()
                .text(&case.question)
                .strategy(SearchStrategy::PathRag {
                    max_hops: 4,
                    sufficiency_threshold: t,
                })
                .k(10)
                .execute()
                .await?;

            let exp_lower = case.expected_answer.to_lowercase();
            let ev_lowers: Vec<String> = case.evidence.iter().map(|e| e.to_lowercase()).collect();

            let ret_5: Vec<_> = res.iter().take(5).collect();
            let ret_10: Vec<_> = res.iter().take(10).collect();

            let is_relevant = |metadata: Option<&serde_json::Value>| {
                if let Some(m) = metadata {
                    if let Some(txt) = m.get("text").and_then(|v| v.as_str()) {
                        let lower = txt.to_lowercase();
                        if (!exp_lower.is_empty() && lower.contains(&exp_lower))
                            || ev_lowers
                                .iter()
                                .any(|ev| !ev.is_empty() && lower.contains(ev))
                        {
                            return true;
                        }
                    }
                }
                false
            };

            let match_count_5 = ret_5
                .iter()
                .filter(|r| is_relevant(r.metadata.as_ref()))
                .count();
            let match_count_10 = ret_10
                .iter()
                .filter(|r| is_relevant(r.metadata.as_ref()))
                .count();

            if match_count_5 > 0 {
                rec5_hits += 1;
            }
            if match_count_10 > 0 {
                rec10_hits += 1;
            }

            let prec_5 = if !ret_5.is_empty() {
                match_count_5 as f64 / ret_5.len() as f64
            } else {
                0.0
            };

            let prec_10 = if !ret_10.is_empty() {
                match_count_10 as f64 / ret_10.len() as f64
            } else {
                0.0
            };

            prec5_sum += prec_5;
            prec10_sum += prec_10;
        }

        let total = eval_cases.len() as f64;
        metrics.push(PathRagSweepMetric {
            threshold: t,
            recall_at_5: if total > 0.0 {
                rec5_hits as f64 / total
            } else {
                0.0
            },
            recall_at_10: if total > 0.0 {
                rec10_hits as f64 / total
            } else {
                0.0
            },
            precision_at_5: if total > 0.0 { prec5_sum / total } else { 0.0 },
            precision_at_10: if total > 0.0 { prec10_sum / total } else { 0.0 },
            total_queries: eval_cases.len(),
        });
    }

    Ok(metrics)
}
