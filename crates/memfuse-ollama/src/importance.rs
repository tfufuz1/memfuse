// FILE-CONTEXT
// STAND: 2026-08-30T18:54:39Z (SESSION: ed7b7b38)
// ZWECK: LLM-basierte Wichtigkeits-Bewertung (ImportanceScore 0.0-1.0) für Memory Chunks
// INVARIANTEN: Parst Floats via OnceLock-Regex; Escaped Input vor Prompt-Bau; Returnt `MemFuseError::Internal` bei Parse-Fehler
// NICHT-OFFENSICHTLICH: Regex parst erste valide 0.0-1.0 Float-Zahl aus LLM Few-Shot Antwort
// HOTSPOTS: score_importance

//! LLM-based Memory Importance scoring using Ollama generate_text.

use crate::client::xml_escape;
use crate::OllamaClient;
use memfuse_core::{ImportanceScore, MemFuseError, Result};
use regex::Regex;
use std::sync::OnceLock;

static SCORE_REGEX: OnceLock<Regex> = OnceLock::new();

/// Confidence or parse status of an LLM importance rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Confidence {
    /// Score was successfully parsed from the LLM output.
    Parsed,
    /// LLM output was unparseable; score defaulted to fallback value (0.5).
    Unparseable,
}

/// Result of an LLM importance evaluation containing the score and parse confidence status.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImportanceAssessment {
    /// The normalized importance score (0.0 to 1.0).
    pub score: ImportanceScore,
    /// Confidence indicator describing whether the score was parsed or defaulted.
    pub confidence: Confidence,
}

impl ImportanceAssessment {
    /// Creates a new `ImportanceAssessment`.
    pub fn new(score: ImportanceScore, confidence: Confidence) -> Self {
        Self { score, confidence }
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
// AI-TAG[ML-SCORING][MAJOR] Score-Konfidenz ohne Kalibrierungsnachweis & Provenienzverlust (APM-22 / APM-24) (ID: AGT-OLLAMA-14c0c140) (TS: 2026-09-06T11:20:14Z) (SESSION: e4f906ee)
// BEFUND: score_importance liefert ein punktuelles Rating (0.0 - 1.0) mit expliziter Parse-Konfidenz (Confidence::Parsed / Confidence::Unparseable).
// RISIKO: Bei Modellwechsel oder unparsbaren Antworten (Fallback 0.5) gehen Drift-Signale und Modellzuordnungen verloren.
// EMPFEHLUNG: Kalibrierungs-Metadaten und Modell-ID in der ImportanceScore-Rückgabe verankern.
pub async fn score_importance(
    client: &OllamaClient,
    chunk_text: &str,
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

    let raw_response = client
        .generate_text(&client.config().model, &prompt)
        .await?;

    let assessment = parse_importance_score_response(&raw_response);
    if assessment.confidence == Confidence::Unparseable {
        tracing::warn!(
            model = %client.config().model,
            "score_importance completed with Confidence::Unparseable fallback"
        );
    }
    Ok(assessment)
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
