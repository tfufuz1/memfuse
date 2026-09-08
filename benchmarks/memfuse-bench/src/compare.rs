// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Baseline-Vergleichs-Engine für Retrieval-Benchmark-Metriken (LongMemEval & LoCoMo)
// INVARIANTEN: Zero Panic, relative Schwellwert-Prüfung, transparente Fehler- & Info-Protokollierung.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LongMemEvalMetricsSummary {
    pub overall_accuracy: f64,
    pub total_cases: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LocomoMetricsSummary {
    pub overall_recall_at_5: f64,
    pub overall_mrr: f64,
    pub total_eval_cases: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CombinedMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_mem_eval: Option<LongMemEvalMetricsSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locomo: Option<LocomoMetricsSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricComparisonResult {
    pub has_regression: bool,
    pub errors: Vec<String>,
    pub info_hints: Vec<String>,
    pub pass_messages: Vec<String>,
}

/// Compares current evaluation metrics against baseline metrics using relative tolerance threshold.
/// `threshold` is a relative fraction (e.g. 0.05 = 5%).
pub fn compare_metrics(
    current: &CombinedMetrics,
    baseline: &CombinedMetrics,
    threshold: f64,
) -> MetricComparisonResult {
    let mut has_regression = false;
    let mut errors = Vec::new();
    let mut info_hints = Vec::new();
    let mut pass_messages = Vec::new();

    // 1. Compare LongMemEval if baseline contains long_mem_eval
    if let Some(base_lme) = &baseline.long_mem_eval {
        if let Some(curr_lme) = &current.long_mem_eval {
            compare_single_metric(
                "long_mem_eval.overall_accuracy",
                curr_lme.overall_accuracy,
                base_lme.overall_accuracy,
                threshold,
                &mut has_regression,
                &mut errors,
                &mut info_hints,
                &mut pass_messages,
            );
        } else {
            has_regression = true;
            errors
                .push("Missing 'long_mem_eval' metrics in current evaluation output.".to_string());
        }
    }

    // 2. Compare LoCoMo if baseline contains locomo
    if let Some(base_locomo) = &baseline.locomo {
        if let Some(curr_locomo) = &current.locomo {
            compare_single_metric(
                "locomo.overall_recall_at_5",
                curr_locomo.overall_recall_at_5,
                base_locomo.overall_recall_at_5,
                threshold,
                &mut has_regression,
                &mut errors,
                &mut info_hints,
                &mut pass_messages,
            );
            compare_single_metric(
                "locomo.overall_mrr",
                curr_locomo.overall_mrr,
                base_locomo.overall_mrr,
                threshold,
                &mut has_regression,
                &mut errors,
                &mut info_hints,
                &mut pass_messages,
            );
        } else {
            has_regression = true;
            errors.push("Missing 'locomo' metrics in current evaluation output.".to_string());
        }
    }

    MetricComparisonResult {
        has_regression,
        errors,
        info_hints,
        pass_messages,
    }
}

#[allow(clippy::too_many_arguments)]
fn compare_single_metric(
    metric_name: &str,
    current_val: f64,
    baseline_val: f64,
    threshold: f64,
    has_regression: &mut bool,
    errors: &mut Vec<String>,
    info_hints: &mut Vec<String>,
    pass_messages: &mut Vec<String>,
) {
    if baseline_val > 0.0 {
        let delta = baseline_val - current_val;
        let relative_drop = delta / baseline_val;

        if current_val < baseline_val && relative_drop > threshold {
            *has_regression = true;
            errors.push(format!(
                "[REGRESSION] {} regressed! Baseline: {:.4}, Current: {:.4} (Drop: {:.2}%, Threshold: {:.2}%)",
                metric_name,
                baseline_val,
                current_val,
                relative_drop * 100.0,
                threshold * 100.0
            ));
        } else if current_val > baseline_val {
            let relative_gain = (current_val - baseline_val) / baseline_val;
            info_hints.push(format!(
                "[INFO] {} EXCEEDS baseline! Baseline: {:.4}, Current: {:.4} (+{:.2}%). Consider updating baseline with: cargo run -p memfuse-bench -- --update-baseline",
                metric_name,
                baseline_val,
                current_val,
                relative_gain * 100.0
            ));
            pass_messages.push(format!(
                "[OK] {} improved: Baseline={:.4}, Current={:.4}",
                metric_name, baseline_val, current_val
            ));
        } else {
            pass_messages.push(format!(
                "[OK] {} within tolerance: Baseline={:.4}, Current={:.4} (Drop: {:.2}% <= {:.2}%)",
                metric_name,
                baseline_val,
                current_val,
                relative_drop.max(0.0) * 100.0,
                threshold * 100.0
            ));
        }
    } else if current_val >= baseline_val {
        pass_messages.push(format!(
            "[OK] {} meets baseline: Baseline={:.4}, Current={:.4}",
            metric_name, baseline_val, current_val
        ));
    } else {
        *has_regression = true;
        errors.push(format!(
            "[REGRESSION] {} regressed from baseline 0.0! Current: {:.4}",
            metric_name, current_val
        ));
    }
}

pub fn compare_metrics_files(
    current_path: &Path,
    baseline_path: &Path,
    threshold: f64,
) -> Result<MetricComparisonResult, String> {
    if !current_path.exists() {
        return Err(format!(
            "Current evaluation metrics file not found at: {}",
            current_path.display()
        ));
    }
    if !baseline_path.exists() {
        return Err(format!(
            "Baseline metrics file not found at: {}",
            baseline_path.display()
        ));
    }

    let current_content = fs::read_to_string(current_path)
        .map_err(|e| format!("Failed to read {}: {}", current_path.display(), e))?;
    let baseline_content = fs::read_to_string(baseline_path)
        .map_err(|e| format!("Failed to read {}: {}", baseline_path.display(), e))?;

    let current: CombinedMetrics = serde_json::from_str(&current_content).map_err(|e| {
        format!(
            "Failed to parse current JSON {}: {}",
            current_path.display(),
            e
        )
    })?;
    let baseline: CombinedMetrics = serde_json::from_str(&baseline_content).map_err(|e| {
        format!(
            "Failed to parse baseline JSON {}: {}",
            baseline_path.display(),
            e
        )
    })?;

    Ok(compare_metrics(&current, &baseline, threshold))
}
