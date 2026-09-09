// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Unit-Tests für Baseline-Vergleichs-Engine (compare_metrics)

use memfuse_bench::compare::{
    compare_metrics, CombinedMetrics, LocomoMetricsSummary, LongMemEvalMetricsSummary,
};

#[test]
fn test_simulated_current_10pp_below_baseline_triggers_regression() {
    let baseline = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.90,
            total_cases: 100,
        }),
        locomo: Some(LocomoMetricsSummary {
            overall_recall_at_5: 0.85,
            overall_mrr: 0.80,
            total_eval_cases: 100,
        }),
    };

    // Current is 10pp below baseline (0.80 vs 0.90 -> drop = 0.10 / 0.90 = 11.1% > 5%)
    let current = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.80,
            total_cases: 100,
        }),
        locomo: Some(LocomoMetricsSummary {
            overall_recall_at_5: 0.85,
            overall_mrr: 0.80,
            total_eval_cases: 100,
        }),
    };

    let res = compare_metrics(&current, &baseline, 0.05);

    assert!(res.has_regression, "10pp drop must trigger regression");
    assert_eq!(res.errors.len(), 1);
    let err_msg = &res.errors[0];
    assert!(err_msg.contains("[REGRESSION]"));
    assert!(err_msg.contains("long_mem_eval.overall_accuracy"));
    assert!(err_msg.contains("0.9000"));
    assert!(err_msg.contains("0.8000"));
}

#[test]
fn test_simulated_current_within_tolerance_passes() {
    let baseline = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.90,
            total_cases: 100,
        }),
        locomo: Some(LocomoMetricsSummary {
            overall_recall_at_5: 0.85,
            overall_mrr: 0.80,
            total_eval_cases: 100,
        }),
    };

    // Current is slightly lower but within 5% relative drop (0.88 vs 0.90 -> drop = 0.02 / 0.90 = 2.22% <= 5%)
    let current = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.88,
            total_cases: 100,
        }),
        locomo: Some(LocomoMetricsSummary {
            overall_recall_at_5: 0.84, // 0.01 / 0.85 = 1.17% <= 5%
            overall_mrr: 0.80,
            total_eval_cases: 100,
        }),
    };

    let res = compare_metrics(&current, &baseline, 0.05);

    assert!(
        !res.has_regression,
        "Slight drop within tolerance must pass"
    );
    assert!(res.errors.is_empty());
    assert!(!res.pass_messages.is_empty());
}

#[test]
fn test_simulated_current_above_baseline_emits_info_no_regression() {
    let baseline = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.80,
            total_cases: 100,
        }),
        locomo: Some(LocomoMetricsSummary {
            overall_recall_at_5: 0.75,
            overall_mrr: 0.70,
            total_eval_cases: 100,
        }),
    };

    // Current outperforms baseline
    let current = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.95,
            total_cases: 100,
        }),
        locomo: Some(LocomoMetricsSummary {
            overall_recall_at_5: 0.85,
            overall_mrr: 0.80,
            total_eval_cases: 100,
        }),
    };

    let res = compare_metrics(&current, &baseline, 0.05);

    assert!(!res.has_regression, "Improved results must pass");
    assert!(res.errors.is_empty());
    assert!(
        !res.info_hints.is_empty(),
        "Info hints must be present when metrics exceed baseline"
    );
    let hint = &res.info_hints[0];
    assert!(hint.contains("EXCEEDS baseline"));
    assert!(hint.contains("cargo run -p memfuse-bench -- --update-baseline"));
}

#[test]
fn test_missing_metric_sections_triggers_regression() {
    let baseline = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.90,
            total_cases: 100,
        }),
        locomo: Some(LocomoMetricsSummary {
            overall_recall_at_5: 0.85,
            overall_mrr: 0.80,
            total_eval_cases: 100,
        }),
    };

    // Current is missing long_mem_eval
    let current_no_lme = CombinedMetrics {
        long_mem_eval: None,
        locomo: Some(LocomoMetricsSummary {
            overall_recall_at_5: 0.85,
            overall_mrr: 0.80,
            total_eval_cases: 100,
        }),
    };

    let res1 = compare_metrics(&current_no_lme, &baseline, 0.05);
    assert!(res1.has_regression);
    assert!(res1
        .errors
        .iter()
        .any(|e| e.contains("Missing 'long_mem_eval' metrics")));

    // Current is missing locomo
    let current_no_locomo = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.90,
            total_cases: 100,
        }),
        locomo: None,
    };

    let res2 = compare_metrics(&current_no_locomo, &baseline, 0.05);
    assert!(res2.has_regression);
    assert!(res2
        .errors
        .iter()
        .any(|e| e.contains("Missing 'locomo' metrics")));
}

#[test]
fn test_zero_baseline_cases() {
    let baseline = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.0,
            total_cases: 0,
        }),
        locomo: None,
    };

    let current_equal = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.0,
            total_cases: 0,
        }),
        locomo: None,
    };

    let res = compare_metrics(&current_equal, &baseline, 0.05);
    assert!(!res.has_regression);
    assert!(res
        .pass_messages
        .iter()
        .any(|m| m.contains("meets baseline")));

    let current_regressed = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: -0.1,
            total_cases: 0,
        }),
        locomo: None,
    };

    let res_err = compare_metrics(&current_regressed, &baseline, 0.05);
    assert!(res_err.has_regression);
    assert!(res_err
        .errors
        .iter()
        .any(|e| e.contains("regressed from baseline 0.0")));
}

#[test]
fn test_compare_metrics_files_errors() {
    use memfuse_bench::compare::compare_metrics_files;
    use std::fs;

    let temp_dir = tempfile::tempdir().unwrap();
    let valid_path = temp_dir.path().join("valid.json");
    let invalid_json_path = temp_dir.path().join("invalid.json");
    let missing_path = temp_dir.path().join("non_existent.json");

    let valid_metrics = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.85,
            total_cases: 10,
        }),
        locomo: None,
    };
    fs::write(&valid_path, serde_json::to_string(&valid_metrics).unwrap()).unwrap();
    fs::write(&invalid_json_path, "{ corrupt_json: ").unwrap();

    // Missing current file
    let err1 = compare_metrics_files(&missing_path, &valid_path, 0.05);
    assert!(err1.is_err());
    assert!(err1
        .unwrap_err()
        .contains("Current evaluation metrics file not found"));

    // Missing baseline file
    let err2 = compare_metrics_files(&valid_path, &missing_path, 0.05);
    assert!(err2.is_err());
    assert!(err2
        .unwrap_err()
        .contains("Baseline metrics file not found"));

    // Corrupt current JSON
    let err3 = compare_metrics_files(&invalid_json_path, &valid_path, 0.05);
    assert!(err3.is_err());
    assert!(err3.unwrap_err().contains("Failed to parse current JSON"));

    // Corrupt baseline JSON
    let err4 = compare_metrics_files(&valid_path, &invalid_json_path, 0.05);
    assert!(err4.is_err());
    assert!(err4.unwrap_err().contains("Failed to parse baseline JSON"));
}

#[test]
fn test_nan_inf_metric_values_trigger_regression() {
    let baseline = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: 0.85,
            total_cases: 100,
        }),
        locomo: None,
    };

    // 1. NaN current value
    let current_nan = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: f64::NAN,
            total_cases: 100,
        }),
        locomo: None,
    };
    let res_nan = compare_metrics(&current_nan, &baseline, 0.05);
    assert!(res_nan.has_regression);
    assert!(res_nan
        .errors
        .iter()
        .any(|e| e.contains("invalid float value (NaN/Inf)")));

    // 2. Infinite current value
    let current_inf = CombinedMetrics {
        long_mem_eval: Some(LongMemEvalMetricsSummary {
            overall_accuracy: f64::INFINITY,
            total_cases: 100,
        }),
        locomo: None,
    };
    let res_inf = compare_metrics(&current_inf, &baseline, 0.05);
    assert!(res_inf.has_regression);
    assert!(res_inf
        .errors
        .iter()
        .any(|e| e.contains("invalid float value (NaN/Inf)")));

    // 3. Negative threshold
    let res_neg_thresh = compare_metrics(&baseline, &baseline, -0.05);
    assert!(res_neg_thresh.has_regression);
    assert!(res_neg_thresh
        .errors
        .iter()
        .any(|e| e.contains("Invalid threshold value")));
}
