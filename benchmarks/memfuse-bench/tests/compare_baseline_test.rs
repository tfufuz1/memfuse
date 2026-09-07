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

    assert!(!res.has_regression, "Slight drop within tolerance must pass");
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
    assert!(!res.info_hints.is_empty(), "Info hints must be present when metrics exceed baseline");
    let hint = &res.info_hints[0];
    assert!(hint.contains("EXCEEDS baseline"));
    assert!(hint.contains("cargo run -p memfuse-bench -- --update-baseline"));
}
