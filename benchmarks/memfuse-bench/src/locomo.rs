// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Loader und Evaluation Harness für das LoCoMo Benchmark (SNAP Research / Long Conversational Memory)
// INVARIANTEN: Lose Kopplung über Search-Closure, keine Panic im Produktionscode, aussagekräftige Fehler.

use crate::long_mem_eval::{BoxFuture, ScoredChunk};
use memfuse_core::{MemFuseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Canonical LoCoMo Question Categories (1 to 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocomoQuestionCategory {
    MultiHop = 1,
    Temporal = 2,
    OpenDomain = 3,
    SingleHop = 4,
    Adversarial = 5,
}

impl LocomoQuestionCategory {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::MultiHop),
            2 => Some(Self::Temporal),
            3 => Some(Self::OpenDomain),
            4 => Some(Self::SingleHop),
            5 => Some(Self::Adversarial),
            _ => None,
        }
    }
}

impl std::fmt::Display for LocomoQuestionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MultiHop => write!(f, "Multi-hop"),
            Self::Temporal => write!(f, "Temporal"),
            Self::OpenDomain => write!(f, "Open-domain"),
            Self::SingleHop => write!(f, "Single-hop"),
            Self::Adversarial => write!(f, "Adversarial"),
        }
    }
}

/// Evaluation test case for LoCoMo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocomoCase {
    pub sample_id: String,
    pub question_id: String,
    pub category: LocomoQuestionCategory,
    pub question: String,
    pub expected_answer: String,
    pub evidence: Vec<String>,
}

/// Report containing evaluation metrics per question category for LoCoMo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocomoReport {
    pub per_category_recall_at_5: HashMap<LocomoQuestionCategory, f64>,
    pub per_category_mrr: HashMap<LocomoQuestionCategory, f64>,
    pub overall_recall_at_5: f64,
    pub overall_mrr: f64,
    pub total_eval_cases: usize,
}

const LOCOMO_URL: &str = "https://github.com/snap-research/locomo";

/// Raw representation of QA item in official `locomo10.json`.
#[derive(Debug, Deserialize)]
struct RawLocomoQaItem {
    question: Option<String>,
    answer: Option<serde_json::Value>,
    adversarial_answer: Option<String>,
    evidence: Option<Vec<String>>,
    category: Option<u8>,
}

/// Raw representation of conversation sample in official `locomo10.json`.
#[derive(Debug, Deserialize)]
struct RawLocomoSample {
    sample_id: Option<String>,
    qa: Option<Vec<RawLocomoQaItem>>,
}

/// Loads LoCoMo cases from `locomo10.json` or custom dataset file.
/// Returns a helpful `Result::Err` if the file does not exist.
pub fn load_locomo_dataset(path: &Path) -> Result<Vec<LocomoCase>> {
    if !path.exists() {
        return Err(MemFuseError::NotFound(format!(
            "LoCoMo dataset file not found at {}. Download locomo10.json from official repository: {}",
            path.display(),
            LOCOMO_URL
        )));
    }

    let file = File::open(path)?;

    let reader = BufReader::new(file);

    let samples: Vec<RawLocomoSample> = serde_json::from_reader(reader).map_err(|e| {
        MemFuseError::Serialization(format!(
            "Failed to parse LoCoMo dataset JSON from {}: {}",
            path.display(),
            e
        ))
    })?;

    let mut cases = Vec::new();

    for sample in samples {
        let sample_id = sample.sample_id.unwrap_or_else(|| "conv_unknown".to_string());
        if let Some(qa_list) = sample.qa {
            for (qa_idx, qa) in qa_list.into_iter().enumerate() {
                let category = qa
                    .category
                    .and_then(LocomoQuestionCategory::from_u8)
                    .unwrap_or(LocomoQuestionCategory::SingleHop);

                let question = match qa.question {
                    Some(q) if !q.trim().is_empty() => q,
                    _ => continue,
                };

                let expected_answer = match qa.answer {
                    Some(serde_json::Value::String(s)) => s,
                    Some(serde_json::Value::Number(n)) => n.to_string(),
                    Some(serde_json::Value::Array(arr)) => arr
                        .into_iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                        .join(" "),
                    _ => qa.adversarial_answer.unwrap_or_default(),
                };

                let evidence = qa.evidence.unwrap_or_default();
                let qid = format!("{}_qa_{}", sample_id, qa_idx + 1);

                cases.push(LocomoCase {
                    sample_id: sample_id.clone(),
                    question_id: qid,
                    category,
                    question,
                    expected_answer,
                    evidence,
                });
            }
        }
    }

    if cases.is_empty() {
        return Err(MemFuseError::InvalidInput(format!(
            "No valid LoCoMo cases extracted from {}. Verify format against official release at {}",
            path.display(),
            LOCOMO_URL
        )));
    }

    Ok(cases)
}

/// Runs LoCoMo benchmark evaluation over dataset cases using an injected search function.
pub async fn run_locomo_eval<'a, F>(cases: &[LocomoCase], search_fn: F) -> Result<LocomoReport>
where
    F: Fn(&str) -> BoxFuture<'a, Result<Vec<ScoredChunk>>>,
{
    // Filter out category 5 (Adversarial) for standard recall/MRR metric aggregation per official benchmark specification
    let eval_cases: Vec<&LocomoCase> = cases
        .iter()
        .filter(|c| c.category != LocomoQuestionCategory::Adversarial)
        .collect();

    let mut category_rec5_hits: HashMap<LocomoQuestionCategory, usize> = HashMap::new();
    let mut category_mrr_sum: HashMap<LocomoQuestionCategory, f64> = HashMap::new();
    let mut category_total: HashMap<LocomoQuestionCategory, usize> = HashMap::new();

    let mut total_rec5_hits = 0;
    let mut total_mrr_sum = 0.0;

    for case in &eval_cases {
        let entry_tot = category_total.entry(case.category).or_insert(0);
        *entry_tot += 1;

        let results = search_fn(&case.question).await?;

        let expected_lower = case.expected_answer.to_lowercase();
        let evidence_lowers: Vec<String> = case.evidence.iter().map(|e| e.to_lowercase()).collect();

        let mut hit_rank: Option<usize> = None;

        for (rank_idx, chunk) in results.iter().enumerate() {
            let chunk_lower = chunk.text.to_lowercase();
            let is_match = (!expected_lower.is_empty() && chunk_lower.contains(&expected_lower))
                || evidence_lowers.iter().any(|ev| !ev.is_empty() && chunk_lower.contains(ev));

            if is_match {
                hit_rank = Some(rank_idx + 1);
                break;
            }
        }

        if let Some(rank) = hit_rank {
            let mrr = 1.0 / (rank as f64);
            let entry_mrr = category_mrr_sum.entry(case.category).or_insert(0.0);
            *entry_mrr += mrr;
            total_mrr_sum += mrr;

            if rank <= 5 {
                let entry_rec = category_rec5_hits.entry(case.category).or_insert(0);
                *entry_rec += 1;
                total_rec5_hits += 1;
            }
        }
    }

    let mut per_category_recall_at_5 = HashMap::new();
    let mut per_category_mrr = HashMap::new();

    for (&cat, &total) in &category_total {
        let hits = category_rec5_hits.get(&cat).copied().unwrap_or(0);
        let mrr_sum = category_mrr_sum.get(&cat).copied().unwrap_or(0.0);

        let rec5 = if total > 0 { hits as f64 / total as f64 } else { 0.0 };
        let mrr = if total > 0 { mrr_sum / total as f64 } else { 0.0 };

        per_category_recall_at_5.insert(cat, rec5);
        per_category_mrr.insert(cat, mrr);
    }

    let n = eval_cases.len();
    let overall_recall_at_5 = if n > 0 { total_rec5_hits as f64 / n as f64 } else { 0.0 };
    let overall_mrr = if n > 0 { total_mrr_sum / n as f64 } else { 0.0 };

    Ok(LocomoReport {
        per_category_recall_at_5,
        per_category_mrr,
        overall_recall_at_5,
        overall_mrr,
        total_eval_cases: n,
    })
}
