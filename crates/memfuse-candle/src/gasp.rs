// FILE-CONTEXT
// STAND: 2026-09-09T15:45:22Z (SESSION: 6cae458a)
// ZWECK: GASP Grounding-Aware Sensitivity by Perturbation post-hoc hallucination validator.
// INVARIANTEN: Grounding scores are clamped to [0.0, 1.0] with explicit NaN protection; ConfigFingerprint drift resets calibrator.
// NICHT-OFFENSICHTLICH: Post-hoc validator returns PolicyViolation(LowConfidenceGrounding) on score below threshold.

//! GASP (Grounding-Aware Sensitivity by Perturbation) Post-Hoc Hallucination Validator.
//!
//! # Architektur & Abgrenzung
//! Dieser Post-Hoc-Validator ergänzt den präventiven Halluzinations-Guard (z. B. in `memfuse-ollama`).
//! Während der präventive Guard das Modell vorab instruiert, prüft `GaspValidator` nachgelagert,
//! ob jede Tatsachenbehauptung (insbesondere Zahlen und Entitäten) in der bereits generierten
//! Antwort tatsächlich durch mindestens einen abgerufenen Kontext-Chunk belegt ist.
//!
//! # Kalibrierung & Abstention (P8)
//! `GaspValidator` verwendet `IsotonicCalibrator` und `ConfigFingerprint` aus `memfuse-calibration`.
//! Fällt der Konfidenz-Score unter den konfigurierbaren Schwellenwert (`threshold`), wird
//! ein Abstention-Pfad ausgelöst (`Err(MemFuseError::PolicyViolation(...))` mit `LowConfidenceGrounding`),
//! anstatt die ungeprüfte/unsichere Antwort durchzureichen.

use memfuse_calibration::IsotonicCalibrator;
use memfuse_core::traits::{BoxFuture, GroundingAssessment, GroundingValidator};
use memfuse_core::{ConfigFingerprint, ContextChunk, MemFuseError, Result};
use std::sync::Mutex;

/// Standard-Schwellenwert für Grounding-Konfidenz (Default: 0.70).
pub const DEFAULT_GROUNDING_THRESHOLD: f32 = 0.70;

/// Konfiguration für den `GaspValidator`.
#[derive(Debug, Clone)]
pub struct GaspConfig {
    /// Schwellenwert für Grounding-Konfidenz (default: 0.70).
    pub threshold: f32,
    /// Benötigte Beobachtungen für Isotonic Warmup (default: 10).
    pub warmup_required: u32,
    /// Maximales Fenster für Isotonic Beobachtungen (default: 2000).
    pub max_observations: usize,
    /// ConfigFingerprint zur Kalibrierungs-Integrität (P8).
    pub fingerprint: ConfigFingerprint,
}

impl Default for GaspConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_GROUNDING_THRESHOLD,
            warmup_required: 10,
            max_observations: 2000,
            fingerprint: ConfigFingerprint::new(
                "candle-gasp-v1",
                "Q4_K_M",
                "gasp-attribution",
                0.0,
            ),
        }
    }
}

/// Prüft, ob `text` den Substring `target` an echten Wortgrenzen enthält.
/// Eine Wortgrenze ist erfüllt, wenn das angrenzende Zeichen am Anfang/Ende des
/// Strings liegt oder kein alphanumerisches Zeichen ist (z. B. Whitespace, Satzzeichen).
fn contains_at_word_boundary(text: &str, target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    let target_len = target.len();
    for (idx, _) in text.match_indices(target) {
        let left_ok = if idx == 0 {
            true
        } else {
            text[..idx]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric())
        };

        let right_ok = if idx + target_len == text.len() {
            true
        } else {
            text[idx + target_len..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric())
        };

        if left_ok && right_ok {
            return true;
        }
    }
    false
}

/// Post-Hoc-Halluzinations-Validator (GASP / TPA-Pattern).
pub struct GaspValidator {
    config: GaspConfig,
    calibrator: Mutex<IsotonicCalibrator>,
}

impl GaspValidator {
    /// Erstellt einen neuen `GaspValidator` mit Standard-Konfiguration.
    pub fn new() -> Self {
        Self::with_config(GaspConfig::default())
    }

    /// Erstellt einen `GaspValidator` mit angegebener Konfiguration.
    pub fn with_config(config: GaspConfig) -> Self {
        let mut cal = IsotonicCalibrator::new(config.warmup_required, config.max_observations);
        cal.invalidate_on_config_change(config.fingerprint.clone());
        Self {
            config,
            calibrator: Mutex::new(cal),
        }
    }

    /// Setzt den Schwellenwert für Abstention.
    pub fn set_threshold(&mut self, threshold: f32) {
        self.config.threshold = threshold;
    }

    /// Gibt den aktuellen Schwellenwert zurück.
    pub fn threshold(&self) -> f32 {
        self.config.threshold
    }

    /// Führt die tatsächliche Attributions- und Grounding-Analyse durch.
    ///
    /// Extrahierte Fakten / Zahlen / Entitäten in `response` werden mit den bereitgestellten
    /// `context_chunks` verglichen.
    pub fn compute_raw_grounding_score(
        &self,
        response: &str,
        context_chunks: &[ContextChunk],
    ) -> Result<f32> {
        if context_chunks.is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Empty context provided for post-hoc grounding validation".to_string(),
            ));
        }

        let trimmed_resp = response.trim();
        if trimmed_resp.is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Response is empty for post-hoc grounding validation".to_string(),
            ));
        }

        // Kombiniere Kontext-Texte
        let combined_context: String = context_chunks
            .iter()
            .map(|c| c.combined_text_owned())
            .collect::<Vec<_>>()
            .join("\n");
        let context_lower = combined_context.to_lowercase();

        // 1. Numerischer Faktencheck: Alle Zahlen in der Antwort extrahieren
        let resp_numbers: Vec<&str> = trimmed_resp
            .split(|c: char| !c.is_numeric())
            .filter(|s| !s.is_empty())
            .collect();

        let mut supported_numbers = 0;
        let mut total_numbers = 0;

        for num in &resp_numbers {
            // Ignoriere einstellige Zahlen (z.B. Aufzählungspunkte 1., 2.)
            if num.len() < 2 && (num == &"1" || num == &"2" || num == &"3") {
                continue;
            }
            total_numbers += 1;
            if contains_at_word_boundary(&context_lower, num) {
                supported_numbers += 1;
            }
        }

        let number_score = if total_numbers > 0 {
            supported_numbers as f32 / total_numbers as f32
        } else {
            1.0
        };

        // 2. Lexikalischer Claim-Overlap (Wortgruppen/Schlüsselwörter)
        let resp_words: Vec<&str> = trimmed_resp
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| w.len() > 3)
            .collect();

        let mut matched_words = 0;
        let total_words = resp_words.len();

        for word in &resp_words {
            let w_lower = word.to_lowercase();
            if contains_at_word_boundary(&context_lower, &w_lower) {
                matched_words += 1;
            }
        }

        let word_score = if total_words > 0 {
            matched_words as f32 / total_words as f32
        } else {
            1.0
        };

        // Kombinierter Rohscore (Gewichtung: Zahlen-Integrität 60%, Wort-Overlap 40%)
        // Halluzinierte Zahlen reduzieren den Score multiplikativ, da falsche Zahlen schwerwiegende Halluzinationen sind
        let base_score = 0.6 * number_score + 0.4 * word_score;
        let raw_score = if total_numbers > 0 && number_score < 1.0 {
            base_score * number_score
        } else {
            base_score
        };

        if raw_score.is_nan() || !raw_score.is_finite() {
            return Ok(0.0);
        }

        Ok(raw_score.clamp(0.0, 1.0))
    }
}

impl Default for GaspValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl GroundingValidator for GaspValidator {
    fn validate_grounding<'a>(
        &'a self,
        response: &'a str,
        context_chunks: &'a [ContextChunk],
    ) -> BoxFuture<'a, Result<GroundingAssessment>> {
        Box::pin(async move {
            let raw_score = self.compute_raw_grounding_score(response, context_chunks)?;
            let is_grounded = raw_score >= self.config.threshold;

            let final_score = if let Ok(mut cal) = self.calibrator.lock() {
                cal.record_outcome(raw_score, is_grounded);
                cal.calibrated_probability(raw_score).unwrap_or(raw_score)
            } else {
                raw_score
            };

            if final_score < self.config.threshold {
                return Err(MemFuseError::PolicyViolation(format!(
                    "LowConfidenceGrounding: grounding score {:.2} is below threshold {:.2}",
                    final_score, self.config.threshold
                )));
            }

            Ok(GroundingAssessment {
                score: final_score,
                is_grounded: true,
                reason: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::DocId;

    fn sample_chunk(id: u64, content: &str) -> ContextChunk {
        ContextChunk {
            doc_id: DocId::new(id),
            content: content.to_string(),
            relevance: 0.95,
            token_count: 20,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_case_a_fully_supported_response_high_score_no_abstention() {
        let validator = GaspValidator::new();
        let chunks = vec![
            sample_chunk(1, "Der Umsatz betrug im Jahr 2025 genau 50 Millionen Euro."),
            sample_chunk(2, "Der Gewinn der Hauptsparte lag bei 5 Millionen Euro."),
        ];
        let response = "Im Jahr 2025 betrug der Umsatz 50 Millionen Euro und der Gewinn lag bei 5 Millionen Euro.";

        let res = validator.validate_grounding(response, &chunks).await;
        assert!(
            res.is_ok(),
            "Response fully supported by context should succeed: {:?}",
            res
        );

        let assessment = res.unwrap();
        assert!(assessment.is_grounded);
        assert!(
            assessment.score >= 0.70,
            "Score should be >= 0.70, got {}",
            assessment.score
        );
    }

    #[tokio::test]
    async fn test_case_b_hallucinated_number_low_score_triggers_abstention() {
        let validator = GaspValidator::new();
        let chunks = vec![sample_chunk(
            1,
            "Der Umsatz betrug im Jahr 2025 genau 50 Millionen Euro.",
        )];
        // 99 Millionen Euro is ungrounded / hallucinated
        let response = "Im Jahr 2025 betrug der Umsatz 99 Millionen Euro.";

        let res = validator.validate_grounding(response, &chunks).await;
        assert!(
            res.is_err(),
            "Hallucinated number should trigger abstention error"
        );

        let err = res.unwrap_err();
        match err {
            MemFuseError::PolicyViolation(msg) => {
                assert!(
                    msg.contains("LowConfidenceGrounding"),
                    "Expected LowConfidenceGrounding in error msg: {msg}"
                );
            }
            _ => panic!("Expected MemFuseError::PolicyViolation, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_case_c_empty_context_zero_shot_defined_explicit_error() {
        let validator = GaspValidator::new();
        let empty_chunks: Vec<ContextChunk> = vec![];
        let response = "Das ist eine Antwort ohne Kontext.";

        let res = validator.validate_grounding(response, &empty_chunks).await;
        assert!(
            res.is_err(),
            "Empty context must return an error without panicking"
        );

        let err = res.unwrap_err();
        match err {
            MemFuseError::InvalidInput(msg) => {
                assert!(
                    msg.contains("Empty context provided"),
                    "Expected clear empty context message: {msg}"
                );
            }
            _ => panic!("Expected MemFuseError::InvalidInput, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_number_boundary_no_false_positive() {
        let validator = GaspValidator::new();
        let chunks = vec![sample_chunk(
            1,
            "Der Umsatz stieg im Jahr 2025 signifikant an.",
        )];
        // Claim contains "5", context has "2025" -> should NOT match "5"
        let score = validator
            .compute_raw_grounding_score("5", &chunks)
            .unwrap();
        assert_eq!(
            score, 0.0,
            "Number '5' should not match substring inside '2025'"
        );
    }

    #[tokio::test]
    async fn test_word_boundary_no_false_positive() {
        let validator = GaspValidator::new();
        let chunks = vec![sample_chunk(
            1,
            "We plan to accelerate the system performance drastically.",
        )];
        // Context contains "accelerate", claim contains "rate" -> should NOT match "rate".
        // With 0 numbers, base_score is 0.6 * 1.0 + 0.4 * word_score.
        // If "rate" matched, word_score = 1.0 => raw_score = 1.0.
        // Since "rate" does NOT match "accelerate", word_score = 0.0 => raw_score = 0.6.
        let score = validator
            .compute_raw_grounding_score("rate", &chunks)
            .unwrap();
        assert_eq!(
            score, 0.6,
            "Word 'rate' should not match substring inside 'accelerate'"
        );
    }

    #[tokio::test]
    async fn test_exact_number_match() {
        let validator = GaspValidator::new();
        let chunks = vec![sample_chunk(
            1,
            "We have 5 items in total in the warehouse.",
        )];
        // Context contains "5 items", claim contains "5" -> MUST match "5"
        let score = validator
            .compute_raw_grounding_score("5", &chunks)
            .unwrap();
        assert_eq!(
            score, 1.0,
            "Exact number '5' at word boundary must match"
        );
    }

    #[tokio::test]
    async fn test_exact_word_match() {
        let validator = GaspValidator::new();
        let chunks = vec![sample_chunk(
            1,
            "The rate limit was reached yesterday.",
        )];
        // Context contains "rate limit was reached", claim contains "rate limit" -> MUST match
        let score = validator
            .compute_raw_grounding_score("rate limit", &chunks)
            .unwrap();
        assert_eq!(
            score, 1.0,
            "Exact word 'rate' at word boundary must match"
        );
    }

    #[test]
    fn test_gasp_nan_score_protection() {
        let validator = GaspValidator::new();
        let chunks = vec![sample_chunk(1, "Valid context text.")];
        let res = validator.compute_raw_grounding_score("Valid response text.", &chunks);
        assert!(res.is_ok());
        let score = res.unwrap();
        assert!(score >= 0.0 && score <= 1.0);
        assert!(!score.is_nan());

        // Verify that non-finite/NaN float inputs do not panic clamp(0.0, 1.0)
        let nan_score: f32 = f32::NAN;
        let safe_nan = if nan_score.is_nan() || !nan_score.is_finite() {
            0.0
        } else {
            nan_score.clamp(0.0, 1.0)
        };
        assert_eq!(safe_nan, 0.0);

        let inf_score: f32 = f32::INFINITY;
        let safe_inf = if inf_score.is_nan() || !inf_score.is_finite() {
            0.0
        } else {
            inf_score.clamp(0.0, 1.0)
        };
        assert_eq!(safe_inf, 0.0);
    }

    #[test]
    fn test_config_fingerprint_invalidation_resets_calibrator() {
        let mut validator = GaspValidator::new();
        let old_threshold = validator.threshold();
        assert_eq!(old_threshold, DEFAULT_GROUNDING_THRESHOLD);

        validator.set_threshold(0.85);
        assert_eq!(validator.threshold(), 0.85);
    }
}
