// FILE-CONTEXT
// STAND: 2026-08-30T18:54:39Z (SESSION: ed7b7b38)
// ZWECK: LLM-basierte Wichtigkeits-Bewertung (ImportanceScore 0.0-1.0) für Memory Chunks
// INVARIANTEN: Parst Floats via OnceLock-Regex; Escaped Input vor Prompt-Bau; Returnt `MemFuseError::Internal` bei Parse-Fehler
// NICHT-OFFENSICHTLICH: Regex parst erste valide 0.0-1.0 Float-Zahl aus LLM Few-Shot Antwort
// HOTSPOTS: score_importance

//! LLM-based Memory Importance scoring using Ollama generate_text.

use crate::client::xml_escape;
use crate::OllamaClient;
use memfuse_calibration::IsotonicCalibrator;
use memfuse_core::{ImportanceScore, MemFuseError, Result};
use parking_lot::Mutex;
use regex::Regex;
use std::sync::{Arc, OnceLock};

static SCORE_REGEX: OnceLock<Regex> = OnceLock::new();

/// Confidence or parse status of an LLM importance rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Confidence {
    /// Score was successfully parsed from the LLM output.
    Parsed,
    /// LLM output was unparseable; score defaulted to fallback value (0.5).
    Unparseable,
}

/// Result of an LLM importance evaluation containing the score, parse confidence status, model provenance, and optional calibrated confidence.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImportanceAssessment {
    /// The normalized importance score (0.0 to 1.0).
    pub score: ImportanceScore,
    /// Confidence indicator describing whether the score was parsed or defaulted.
    pub confidence: Confidence,
    /// Model ID used to generate this score (prevents provenance loss across model changes).
    #[serde(default)]
    pub model_id: String,
    /// Calibrated probability when `IsotonicCalibrator` / `PlattScaler` has completed warmup.
    /// `None` when calibration is unavailable (explicit, no silent fallback).
    #[serde(default)]
    pub calibrated_confidence: Option<f32>,
}

impl ImportanceAssessment {
    /// Creates a new `ImportanceAssessment` with default empty model ID and no calibration.
    pub fn new(score: ImportanceScore, confidence: Confidence) -> Self {
        Self {
            score,
            confidence,
            model_id: String::new(),
            calibrated_confidence: None,
        }
    }

    /// Creates a new `ImportanceAssessment` with model provenance and optional calibrated confidence.
    pub fn with_provenance(
        score: ImportanceScore,
        confidence: Confidence,
        model_id: impl Into<String>,
        calibrated_confidence: Option<f32>,
    ) -> Self {
        Self {
            score,
            confidence,
            model_id: model_id.into(),
            calibrated_confidence,
        }
    }

    /// Helper returning the raw `f32` importance score value.
    pub fn value(&self) -> f32 {
        self.score.value()
    }

    /// Returns `true` if the score was parsed successfully from the LLM output.
    pub fn is_parsed(&self) -> bool {
        self.confidence == Confidence::Parsed
    }
}

fn get_score_regex() -> Result<&'static Regex> {
    if let Some(re) = SCORE_REGEX.get() {
        return Ok(re);
    }
    let re = Regex::new(r"(?:0(?:\.\d+)?|1(?:\.0+)?|\.\d+)")
        .map_err(|e| MemFuseError::Internal(format!("Regex compilation failed: {e}")))?;
    let _ = SCORE_REGEX.set(re);
    SCORE_REGEX
        .get()
        .ok_or_else(|| MemFuseError::Internal("SCORE_REGEX set failed".into()))
}

/// Evaluates the importance of a text chunk using a local Ollama LLM model.
///
/// Uses `OllamaClient::generate_text()` with a strict Few-Shot prompt forcing
/// a score output between 0.0 and 1.0.
///
/// # Errors
/// - Returns `MemFuseError::InvalidInput` if `chunk_text` is empty.
/// - Returns `MemFuseError::Storage` / `MemFuseError::Io` on network or Ollama API errors.
// AI-TAG[ML-SCORING][MAJOR] RESOLVED: Score importance enriches output with model_id provenance and optional calibrated_confidence via IsotonicCalibrator. Post-hoc outcome feedback interface record_importance_outcome added (ID: AGT-OLLAMA-14c0c140) (TS: 2026-09-07T06:00:00Z) (SESSION: jules)
pub async fn score_importance(
    client: &OllamaClient,
    chunk_text: &str,
) -> Result<ImportanceAssessment> {
    score_importance_with_calibrator(client, chunk_text, None).await
}

/// Evaluates importance of a text chunk with an optional shared `IsotonicCalibrator`.
///
/// Populates `model_id` provenance from `client.config().model` and enriches
/// `calibrated_confidence` when the calibrator has completed warmup.
pub async fn score_importance_with_calibrator(
    client: &OllamaClient,
    chunk_text: &str,
    calibrator: Option<&Arc<Mutex<IsotonicCalibrator>>>,
) -> Result<ImportanceAssessment> {
    if chunk_text.trim().is_empty() {
        return Err(MemFuseError::InvalidInput(
            "chunk_text must not be empty".into(),
        ));
    }

    let escaped = xml_escape(chunk_text);

    let prompt = format!(
        "Rate the long-term importance of the following memory chunk for an AI agent on a scale from 0.0 to 1.0.\n\
         - 0.0 = trivial, ephemeral, chatter, or irrelevant noise.\n\
         - 0.5 = moderate utility, general background context.\n\
         - 1.0 = critical fact, user preference, core identity, or key security credential.\n\n\
         Few-Shot Examples:\n\
         Memory: 'Hello, how are you today?' -> 0.1\n\
         Memory: 'The weather in Berlin is 22C.' -> 0.3\n\
         Memory: 'User prefers Rust code examples over Python.' -> 0.9\n\
         Memory: 'System password hint is super-secret-123.' -> 1.0\n\n\
         Memory to rate:\n\
         \"{escaped}\"\n\n\
         Return ONLY a single floating-point number between 0.0 and 1.0. No explanations or extra text."
    );

    let model_id = client.config().model.clone();
    let raw_response = client.generate_text(&model_id, &prompt).await?;

    let mut assessment = parse_importance_score_response(&raw_response);
    assessment.model_id = model_id.clone();

    if let Some(cal) = calibrator {
        assessment.calibrated_confidence = cal.lock().calibrated_probability(assessment.value());
    }

    if assessment.confidence == Confidence::Unparseable {
        tracing::warn!(
            model = %model_id,
            "score_importance completed with Confidence::Unparseable fallback"
        );
    }
    Ok(assessment)
}

/// Records outcome feedback for an importance score into an `IsotonicCalibrator`.
///
/// Intended for downstream retrieval feedback loops evaluating whether a memory chunk
/// assigned a given `raw_score` was actually useful during agent operations.
pub fn record_importance_outcome(
    calibrator: &Arc<Mutex<IsotonicCalibrator>>,
    raw_score: f32,
    was_useful: bool,
) {
    calibrator.lock().record_outcome(raw_score, was_useful);
}

/// Parses an `ImportanceAssessment` from raw LLM output.
/// Returns `Confidence::Parsed` on successful float extraction,
/// or `Confidence::Unparseable` with default score (0.5) and a warning log if unparseable.
pub fn parse_importance_score_response(raw_response: &str) -> ImportanceAssessment {
    let Ok(re) = get_score_regex() else {
        let truncated: String = raw_response.chars().take(200).collect();
        tracing::warn!(
            raw_response = %truncated,
            "SCORE_REGEX compilation failed, returning default ImportanceScore(0.5) with Confidence::Unparseable"
        );
        return ImportanceAssessment::new(ImportanceScore::default(), Confidence::Unparseable);
    };

    let trimmed = raw_response.trim();
    if let Some(m) = re.find(trimmed) {
        let matched = m.as_str();
        if let Ok(parsed) = matched.parse::<f32>() {
            return ImportanceAssessment::new(ImportanceScore::new(parsed), Confidence::Parsed);
        }
    }

    let truncated: String = raw_response.chars().take(200).collect();
    tracing::warn!(
        raw_response = %truncated,
        "Failed to parse ImportanceScore float from LLM response, returning default ImportanceScore(0.5) with Confidence::Unparseable"
    );
    ImportanceAssessment::new(ImportanceScore::default(), Confidence::Unparseable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_score_importance_empty_text_error() {
        let client = OllamaClient::new("http://localhost:11434");
        let res = score_importance(&client, "   ").await;
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_score_importance_mock_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(); // unwrap
        let addr = listener.local_addr().unwrap(); // unwrap
        let server_url = format!("http://{}", addr);

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = serde_json::json!({
                    "message": {
                        "role": "assistant",
                        "content": " Based on evaluation: 0.85 (High importance)"
                    }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.ok();
            }
        });

        let client = OllamaClient::new(server_url);
        let assessment = score_importance(&client, "User prefers dark mode.")
            .await
            .unwrap(); // unwrap
        assert_eq!(assessment.value(), 0.85);
        assert_eq!(assessment.confidence, Confidence::Parsed);
        assert!(assessment.is_parsed());
    }

    #[tokio::test]
    async fn test_score_importance_mock_invalid_response_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(); // unwrap
        let addr = listener.local_addr().unwrap(); // unwrap
        let server_url = format!("http://{}", addr);

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = serde_json::json!({
                    "message": {
                        "role": "assistant",
                        "content": "I am not able to rate this memory."
                    }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.ok();
            }
        });

        let client = OllamaClient::new(server_url);
        let res = score_importance(&client, "Some chunk text").await;
        assert!(res.is_ok());
        let assessment = res.unwrap(); // unwrap
        assert_eq!(assessment.value(), 0.5);
        assert_eq!(assessment.confidence, Confidence::Unparseable);
        assert!(!assessment.is_parsed());
    }

    #[test]
    fn test_score_importance_regex_get() {
        let re_res = get_score_regex();
        assert!(re_res.is_ok());
        let re = re_res.unwrap(); // unwrap
        assert!(re.is_match("0.85"));
        assert!(re.is_match("1.0"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn test_score_importance_regex_parsing_edge_cases() {
        let s1 = parse_importance_score_response("Based on analysis: 0.75");
        assert_eq!(s1.value(), 0.75);
        assert_eq!(s1.confidence, Confidence::Parsed);

        let s2 = parse_importance_score_response("Score: 1.0");
        assert_eq!(s2.value(), 1.0);
        assert_eq!(s2.confidence, Confidence::Parsed);

        let s3 = parse_importance_score_response("Rating is 0");
        assert_eq!(s3.value(), 0.0);
        assert_eq!(s3.confidence, Confidence::Parsed);

        let s4 = parse_importance_score_response("Importance = .42 (Moderate)");
        assert_eq!(s4.value(), 0.42);
        assert_eq!(s4.confidence, Confidence::Parsed);

        let s5 = parse_importance_score_response("Relevanz: 0.8/1.0");
        assert_eq!(s5.value(), 0.8);
        assert_eq!(s5.confidence, Confidence::Parsed);

        let s6 = parse_importance_score_response("Unparseable garbage response text");
        assert_eq!(s6.value(), 0.5);
        assert_eq!(s6.confidence, Confidence::Unparseable);
    }

    #[test]
    fn test_unparseable_response_returns_unparseable_confidence() {
        let res =
            parse_importance_score_response("I cannot rate this memory text without context.");
        assert_eq!(res.value(), 0.5);
        assert_eq!(res.confidence, Confidence::Unparseable);
        assert!(!res.is_parsed());
    }

    #[test]
    fn test_parsed_0_5_score_returns_parsed_confidence() {
        let res = parse_importance_score_response("Importance rating: 0.5");
        assert_eq!(res.value(), 0.5);
        assert_eq!(res.confidence, Confidence::Parsed);
        assert!(res.is_parsed());
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_unparseable_response_logs_warning_with_raw_text() {
        let raw = "Completely unparseable model response string";
        let res = parse_importance_score_response(raw);
        assert_eq!(res.confidence, Confidence::Unparseable);
        assert!(logs_contain(
            "Failed to parse ImportanceScore float from LLM response"
        ));
        assert!(logs_contain("Completely unparseable model response string"));
    }

    #[tokio::test]
    async fn test_score_importance_with_calibrator_provenance_and_warmup() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(); // unwrap
        let addr = listener.local_addr().unwrap(); // unwrap
        let server_url = format!("http://{}", addr);

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = serde_json::json!({
                    "message": {
                        "role": "assistant",
                        "content": "0.80"
                    }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.ok();
            }
        });

        let client = OllamaClient::new(server_url);
        let calibrator = Arc::new(Mutex::new(IsotonicCalibrator::new(5, 100)));

        // 1. Before warmup threshold (5 observations), calibrated_confidence should be None
        let assessment_before =
            score_importance_with_calibrator(&client, "Chunk text 1", Some(&calibrator))
                .await
                .unwrap(); // unwrap

        assert_eq!(assessment_before.value(), 0.80);
        assert_eq!(assessment_before.confidence, Confidence::Parsed);
        assert_eq!(assessment_before.model_id, client.config().model);
        assert_eq!(assessment_before.calibrated_confidence, None);

        // 2. Inject outcome feedback via record_importance_outcome until warmup threshold is reached
        for i in 0..10 {
            record_importance_outcome(&calibrator, i as f32 / 10.0, i > 4);
        }

        // 3. After warmup threshold, calibrated_confidence should yield Some(f32)
        let assessment_after =
            score_importance_with_calibrator(&client, "Chunk text 2", Some(&calibrator))
                .await
                .unwrap(); // unwrap

        assert_eq!(assessment_after.value(), 0.80);
        assert_eq!(assessment_after.model_id, client.config().model);
        assert!(assessment_after.calibrated_confidence.is_some());
        let cal_prob = assessment_after.calibrated_confidence.unwrap(); // unwrap
        assert!((0.0..=1.0).contains(&cal_prob));
    }

    #[test]
    fn test_importance_assessment_serde_backward_compatibility() {
        // Legacy JSON without model_id and calibrated_confidence fields
        let json_legacy = r#"{"score":0.75,"confidence":"Parsed"}"#;
        let deser: ImportanceAssessment = serde_json::from_str(json_legacy).unwrap(); // unwrap

        assert_eq!(deser.value(), 0.75);
        assert_eq!(deser.confidence, Confidence::Parsed);
        assert_eq!(deser.model_id, "");
        assert_eq!(deser.calibrated_confidence, None);

        // New JSON with all fields
        let json_new = r#"{"score":0.90,"confidence":"Parsed","model_id":"llama3","calibrated_confidence":0.88}"#;
        let deser_new: ImportanceAssessment = serde_json::from_str(json_new).unwrap(); // unwrap

        assert_eq!(deser_new.value(), 0.90);
        assert_eq!(deser_new.confidence, Confidence::Parsed);
        assert_eq!(deser_new.model_id, "llama3");
        assert_eq!(deser_new.calibrated_confidence, Some(0.88));
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]
        #[test]
        fn prop_score_importance_parse_formats(
            prefix in "[a-zA-Z0-9 :\\-_/]{0,30}",
            score_str in "(0|1|0\\.[0-9]{1,4}|1\\.0{1,4}|\\.[0-9]{1,4})",
            suffix in "[a-zA-Z0-9 :\\-_/]{0,30}",
        ) {
            let llm_output = format!("{}{}{}", prefix, score_str, suffix);
            let parsed = parse_importance_score_response(&llm_output);
            assert!(!parsed.value().is_nan());
            assert!((0.0..=1.0).contains(&parsed.value()));
            assert_eq!(parsed.confidence, Confidence::Parsed);
        }

        #[test]
        fn prop_score_importance_arbitrary_garbage_never_panics(
            garbage in ".*",
        ) {
            let parsed = parse_importance_score_response(&garbage);
            assert!(!parsed.value().is_nan());
            assert!((0.0..=1.0).contains(&parsed.value()));
        }
    }
}
