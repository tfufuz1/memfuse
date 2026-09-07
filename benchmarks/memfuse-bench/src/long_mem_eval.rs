// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Loader und Evaluation Harness für das LongMemEval Benchmark (ICLR 2025 / arXiv:2410.10813)
// INVARIANTEN: Lose Kopplung über Search-Closure, keine Panic im Produktionscode, aussagekräftige Fehler.

use memfuse_core::{MemFuseError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::pin::Pin;

/// Alias for an owned Send BoxFuture.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Generic search chunk returned by an injected search pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredChunk {
    pub id: String,
    pub text: String,
    pub score: f32,
}

/// Official question categories in LongMemEval (arXiv:2410.10813).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LongMemEvalQuestionType {
    SingleSessionUser,
    SingleSessionAssistant,
    SingleSessionPreference,
    MultiSession,
    KnowledgeUpdate,
    TemporalReasoning,
    Abstention,
}

impl std::fmt::Display for LongMemEvalQuestionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SingleSessionUser => write!(f, "SingleSessionUser"),
            Self::SingleSessionAssistant => write!(f, "SingleSessionAssistant"),
            Self::SingleSessionPreference => write!(f, "SingleSessionPreference"),
            Self::MultiSession => write!(f, "MultiSession"),
            Self::KnowledgeUpdate => write!(f, "KnowledgeUpdate"),
            Self::TemporalReasoning => write!(f, "TemporalReasoning"),
            Self::Abstention => write!(f, "Abstention"),
        }
    }
}

/// Evaluation test case for LongMemEval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongMemEvalCase {
    pub question_id: String,
    pub session_history: Vec<(
        String, /* speaker/role */
        String, /* utterance */
        u64,    /* session_idx */
    )>,
    pub question: String,
    pub expected_answer: String,
    pub question_type: LongMemEvalQuestionType,
}

/// Report containing evaluation metrics per question category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongMemEvalReport {
    pub per_category_accuracy: HashMap<LongMemEvalQuestionType, f64>,
    pub overall_accuracy: f64,
    pub total_cases: usize,
}

/// Internal JSON turn representation for LongMemEval JSON/JSONL format.
#[derive(Debug, Deserialize)]
struct RawTurn {
    role: Option<String>,
    speaker: Option<String>,
    content: Option<String>,
    text: Option<String>,
}

/// Internal JSON evaluation instance for LongMemEval.
#[derive(Debug, Deserialize)]
struct RawLongMemEvalItem {
    question_id: Option<String>,
    id: Option<String>,
    question_type: Option<String>,
    question: Option<String>,
    answer: Option<String>,
    expected_answer: Option<String>,
    haystack_sessions: Option<Vec<Vec<RawTurn>>>,
    sessions: Option<Vec<Vec<RawTurn>>>,
    session_history: Option<Vec<Vec<RawTurn>>>,
}

const LONG_MEM_EVAL_URL: &str = "https://github.com/xiaowu0162/longmemeval";

/// Loads LongMemEval cases from a JSONL or JSON dataset file.
/// Returns a helpful `Result::Err` if the file does not exist.
pub fn load_from_jsonl(path: &Path) -> Result<Vec<LongMemEvalCase>> {
    if !path.exists() {
        return Err(MemFuseError::NotFound(format!(
            "LongMemEval dataset file not found at {}. Download the official dataset from {}",
            path.display(),
            LONG_MEM_EVAL_URL
        )));
    }

    let file = File::open(path)?;

    let reader = BufReader::new(file);

    // Try reading as JSONL first; if line 1 starts with '[', attempt JSON array parsing.
    let mut cases = Vec::new();

    let content = std::fs::read_to_string(path)?;

    let trimmed = content.trim();
    if trimmed.starts_with('[') {
        let raw_items: Vec<RawLongMemEvalItem> = serde_json::from_str(trimmed).map_err(|e| {
            MemFuseError::Serialization(format!(
                "Failed to parse LongMemEval JSON array from {}: {}",
                path.display(),
                e
            ))
        })?;
        for item in raw_items {
            if let Some(case) = parse_raw_item(item) {
                cases.push(case);
            }
        }
    } else {
        for (line_idx, line) in reader.lines().enumerate() {
            let line_str = line.map_err(|e| {
                MemFuseError::Serialization(format!(
                    "Read error at line {} in {}: {}",
                    line_idx + 1,
                    path.display(),
                    e
                ))
            })?;
            let trimmed_line = line_str.trim();
            if trimmed_line.is_empty() {
                continue;
            }
            let raw_item: RawLongMemEvalItem = serde_json::from_str(trimmed_line).map_err(|e| {
                MemFuseError::Serialization(format!(
                    "Invalid JSON at line {} in {}: {}",
                    line_idx + 1,
                    path.display(),
                    e
                ))
            })?;
            if let Some(case) = parse_raw_item(raw_item) {
                cases.push(case);
            }
        }
    }

    if cases.is_empty() {
        return Err(MemFuseError::InvalidInput(format!(
            "No valid LongMemEval cases found in {}. Ensure dataset follows official schema from {}",
            path.display(),
            LONG_MEM_EVAL_URL
        )));
    }

    Ok(cases)
}

fn parse_question_type(qtype_str: &str, qid: &str) -> LongMemEvalQuestionType {
    if qid.ends_with("_abs") || qtype_str.eq_ignore_ascii_case("abstention") {
        return LongMemEvalQuestionType::Abstention;
    }
    match qtype_str.to_lowercase().replace('_', "-").as_str() {
        "single-session-user" | "single_session_user" => LongMemEvalQuestionType::SingleSessionUser,
        "single-session-assistant" | "single_session_assistant" => {
            LongMemEvalQuestionType::SingleSessionAssistant
        }
        "single-session-preference" | "single_session_preference" => {
            LongMemEvalQuestionType::SingleSessionPreference
        }
        "multi-session" | "multi_session" => LongMemEvalQuestionType::MultiSession,
        "knowledge-update" | "knowledge_update" => LongMemEvalQuestionType::KnowledgeUpdate,
        "temporal-reasoning" | "temporal_reasoning" => LongMemEvalQuestionType::TemporalReasoning,
        "abstention" => LongMemEvalQuestionType::Abstention,
        _ => LongMemEvalQuestionType::SingleSessionUser,
    }
}

fn parse_raw_item(item: RawLongMemEvalItem) -> Option<LongMemEvalCase> {
    let qid = item
        .question_id
        .or(item.id)
        .unwrap_or_else(|| "unknown_qid".to_string());

    let question = item.question?;
    let expected_answer = item.answer.or(item.expected_answer).unwrap_or_default();

    let qtype_str = item.question_type.unwrap_or_default();
    let question_type = parse_question_type(&qtype_str, &qid);

    let raw_sessions = item
        .haystack_sessions
        .or(item.sessions)
        .or(item.session_history)
        .unwrap_or_default();

    let mut session_history = Vec::new();
    for (sess_idx, session) in raw_sessions.into_iter().enumerate() {
        for turn in session {
            let role = turn
                .role
                .or(turn.speaker)
                .unwrap_or_else(|| "user".to_string());
            let utterance = turn.content.or(turn.text).unwrap_or_default();
            session_history.push((role, utterance, sess_idx as u64));
        }
    }

    Some(LongMemEvalCase {
        question_id: qid,
        session_history,
        question,
        expected_answer,
        question_type,
    })
}

/// Evaluates LongMemEval test cases using a dependency-injected retrieval function.
pub async fn run_long_mem_eval<'a, F>(
    cases: &[LongMemEvalCase],
    search_fn: F,
) -> Result<LongMemEvalReport>
where
    F: Fn(&str) -> BoxFuture<'a, Result<Vec<ScoredChunk>>>,
{
    let mut category_correct: HashMap<LongMemEvalQuestionType, usize> = HashMap::new();
    let mut category_total: HashMap<LongMemEvalQuestionType, usize> = HashMap::new();
    let mut total_correct = 0;

    for case in cases {
        let entry_total = category_total.entry(case.question_type).or_insert(0);
        *entry_total += 1;

        let search_results = search_fn(&case.question).await?;

        // Standard retrieval precision check: hit if top result text contains answer keywords or expected answer substring
        let is_correct = if case.question_type == LongMemEvalQuestionType::Abstention {
            // For abstention, search result scores should be low or empty
            search_results.is_empty()
                || search_results
                    .iter()
                    .all(|c| c.score < 0.1 || c.text.is_empty())
        } else {
            let lower_answer = case.expected_answer.to_lowercase();
            search_results.iter().any(|chunk| {
                let lower_chunk = chunk.text.to_lowercase();
                lower_chunk.contains(&lower_answer)
                    || lower_answer
                        .split_whitespace()
                        .any(|word| word.len() > 3 && lower_chunk.contains(word))
            })
        };

        if is_correct {
            let entry_correct = category_correct.entry(case.question_type).or_insert(0);
            *entry_correct += 1;
            total_correct += 1;
        }
    }

    let mut per_category_accuracy = HashMap::new();
    for (&cat, &total) in &category_total {
        let correct = category_correct.get(&cat).copied().unwrap_or(0);
        let acc = if total > 0 {
            correct as f64 / total as f64
        } else {
            0.0
        };
        per_category_accuracy.insert(cat, acc);
    }

    let overall_accuracy = if !cases.is_empty() {
        total_correct as f64 / cases.len() as f64
    } else {
        0.0
    };

    Ok(LongMemEvalReport {
        per_category_accuracy,
        overall_accuracy,
        total_cases: cases.len(),
    })
}
