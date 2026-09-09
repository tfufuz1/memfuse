// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Tests für LongMemEval, LoCoMo und RegressionSuite Benchmark-Module

use memfuse_bench::locomo::{load_locomo_dataset, run_locomo_eval, LocomoQuestionCategory};
use memfuse_bench::long_mem_eval::{
    check_regression, load_from_jsonl, run_long_mem_eval, LongMemEvalQuestionType,
    RegressionReport, RegressionSuite, ScoredChunk,
};
use memfuse_bench::path_rag_sweep::{run_pathrag_sweep_locomo, run_pathrag_sweep_long_mem_eval};
use memfuse_core::Result;
use memfuse_db::{MemFuse, MemFuseConfig};
use std::path::Path;
use tempfile::TempDir;

#[tokio::test]
async fn test_long_mem_eval_fixture_parsing_and_eval() {
    let fixture_path = Path::new("tests/fixtures/long_mem_eval_fixture.json");
    let cases = load_from_jsonl(fixture_path).expect("Fixture should parse successfully");

    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].question_id, "test_q1");
    assert_eq!(
        cases[0].question_type,
        LongMemEvalQuestionType::SingleSessionUser
    );
    assert_eq!(cases[1].question_type, LongMemEvalQuestionType::Abstention);

    let mock_search = |q: &str| {
        let q_text = q.to_string();
        Box::pin(async move {
            if q_text.contains("drink") {
                Ok(vec![ScoredChunk {
                    id: "doc1".into(),
                    text: "I really love Earl Grey tea in the morning.".into(),
                    score: 0.95,
                }])
            } else {
                Ok(vec![])
            }
        })
            as memfuse_bench::long_mem_eval::BoxFuture<
                'static,
                memfuse_core::Result<Vec<ScoredChunk>>,
            >
    };

    let report = run_long_mem_eval(&cases, mock_search)
        .await
        .expect("Evaluation should succeed");

    assert_eq!(report.total_cases, 2);
    assert_eq!(report.overall_accuracy, 1.0);
    assert_eq!(
        report
            .per_category_accuracy
            .get(&LongMemEvalQuestionType::SingleSessionUser),
        Some(&1.0)
    );
    assert_eq!(
        report
            .per_category_accuracy
            .get(&LongMemEvalQuestionType::Abstention),
        Some(&1.0)
    );
}

#[tokio::test]
async fn test_locomo_fixture_parsing_and_eval() {
    let fixture_path = Path::new("tests/fixtures/locomo_fixture.json");
    let cases = load_locomo_dataset(fixture_path).expect("Fixture should parse successfully");

    assert_eq!(cases.len(), 3);

    let mock_search = |q: &str| {
        let q_text = q.to_string();
        Box::pin(async move {
            if q_text.contains("Berlin") {
                Ok(vec![ScoredChunk {
                    id: "chunk_1".into(),
                    text: "Alice visited Berlin in May 2023 with friends.".into(),
                    score: 0.92,
                }])
            } else {
                Ok(vec![ScoredChunk {
                    id: "chunk_2".into(),
                    text: "Paris is capital of France.".into(),
                    score: 0.88,
                }])
            }
        })
            as memfuse_bench::long_mem_eval::BoxFuture<
                'static,
                memfuse_core::Result<Vec<ScoredChunk>>,
            >
    };

    let report = run_locomo_eval(&cases, mock_search)
        .await
        .expect("Evaluation should succeed");

    // Category 5 (Adversarial) is excluded from eval count per specification (2 cases remain)
    assert_eq!(report.total_eval_cases, 2);
    assert_eq!(report.overall_recall_at_5, 1.0);
    assert_eq!(report.overall_mrr, 1.0);
    assert_eq!(
        report
            .per_category_recall_at_5
            .get(&LocomoQuestionCategory::SingleHop),
        Some(&1.0)
    );
}

#[test]
fn test_missing_file_returns_helpful_error_no_panic() {
    let missing_path = Path::new("non_existent_dataset_dir/file.jsonl");

    let long_mem_res = load_from_jsonl(missing_path);
    assert!(long_mem_res.is_err());
    let long_mem_err = long_mem_res.unwrap_err().to_string();
    assert!(long_mem_err.contains("LongMemEval dataset file not found"));
    assert!(long_mem_err.contains("https://github.com/xiaowu0162/longmemeval"));

    let locomo_res = load_locomo_dataset(missing_path);
    assert!(locomo_res.is_err());
    let locomo_err = locomo_res.unwrap_err().to_string();
    assert!(locomo_err.contains("LoCoMo dataset file not found"));
    assert!(locomo_err.contains("https://github.com/snap-research/locomo"));
}

#[tokio::test]
async fn test_regression_suite_baseline_count_and_execution() -> Result<()> {
    let suite = RegressionSuite::baseline();
    assert!(
        suite.scenarios.len() >= 30,
        "Regression suite must contain at least 30 scenarios, found {}",
        suite.scenarios.len()
    );

    let temp_dir = TempDir::new()?;
    let db_cfg = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(temp_dir.path(), db_cfg).await?;
    let col = db.collection("test_regression_col").await?;

    let report = suite.run_against_collection(&col).await?;
    assert_eq!(report.total_scenarios, suite.scenarios.len());
    assert!(
        report.recall_at_5 >= 0.8,
        "Baseline Recall@5 should be high (>= 80%), got {:.3}",
        report.recall_at_5
    );

    Ok(())
}

#[tokio::test]
async fn test_pathrag_sweep_long_mem_eval_execution() -> Result<()> {
    // Standard execution with 2 thresholds
    let thresholds = vec![0.1, 0.5];
    let sweep_results = run_pathrag_sweep_long_mem_eval(&thresholds).await?;

    assert_eq!(sweep_results.len(), 2);
    assert_eq!(sweep_results[0].threshold, 0.1);
    assert_eq!(sweep_results[1].threshold, 0.5);
    assert!(sweep_results[0].total_queries >= 30);

    // Empty thresholds
    let empty_sweep = run_pathrag_sweep_long_mem_eval(&[]).await?;
    assert!(empty_sweep.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_pathrag_sweep_locomo_execution() -> Result<()> {
    let fixture_path = Path::new("tests/fixtures/locomo_fixture.json");
    let thresholds = vec![0.1, 0.5];

    let sweep_results = run_pathrag_sweep_locomo(fixture_path, &thresholds).await?;
    assert_eq!(sweep_results.len(), 2);
    assert_eq!(sweep_results[0].threshold, 0.1);
    assert_eq!(sweep_results[1].threshold, 0.5);
    // Fixture has 3 total cases, 1 adversarial excluded -> 2 eval cases
    assert_eq!(sweep_results[0].total_queries, 2);

    // Empty thresholds
    let empty_sweep = run_pathrag_sweep_locomo(fixture_path, &[]).await?;
    assert!(empty_sweep.is_empty());

    // Non-existent dataset file error propagation
    let missing_path = Path::new("non_existent_dataset_dir/locomo.json");
    let err_res = run_pathrag_sweep_locomo(missing_path, &thresholds).await;
    assert!(err_res.is_err());
    let err_msg = err_res.unwrap_err().to_string();
    assert!(err_msg.contains("LoCoMo dataset file not found"));

    Ok(())
}

#[test]
fn test_locomo_category_enum_conversions() {
    assert_eq!(
        LocomoQuestionCategory::from_u8(1),
        Some(LocomoQuestionCategory::MultiHop)
    );
    assert_eq!(
        LocomoQuestionCategory::from_u8(2),
        Some(LocomoQuestionCategory::Temporal)
    );
    assert_eq!(
        LocomoQuestionCategory::from_u8(3),
        Some(LocomoQuestionCategory::OpenDomain)
    );
    assert_eq!(
        LocomoQuestionCategory::from_u8(4),
        Some(LocomoQuestionCategory::SingleHop)
    );
    assert_eq!(
        LocomoQuestionCategory::from_u8(5),
        Some(LocomoQuestionCategory::Adversarial)
    );
    assert_eq!(LocomoQuestionCategory::from_u8(0), None);
    assert_eq!(LocomoQuestionCategory::from_u8(6), None);
    assert_eq!(LocomoQuestionCategory::from_u8(255), None);

    assert_eq!(LocomoQuestionCategory::MultiHop.to_string(), "Multi-hop");
    assert_eq!(LocomoQuestionCategory::Temporal.to_string(), "Temporal");
    assert_eq!(
        LocomoQuestionCategory::OpenDomain.to_string(),
        "Open-domain"
    );
    assert_eq!(LocomoQuestionCategory::SingleHop.to_string(), "Single-hop");
    assert_eq!(
        LocomoQuestionCategory::Adversarial.to_string(),
        "Adversarial"
    );
}

#[test]
fn test_long_mem_eval_question_type_enum_conversions() {
    assert_eq!(
        format!("{:?}", LongMemEvalQuestionType::SingleSessionUser),
        "SingleSessionUser"
    );
    assert_eq!(
        format!("{:?}", LongMemEvalQuestionType::SingleSessionAssistant),
        "SingleSessionAssistant"
    );
    assert_eq!(
        format!("{:?}", LongMemEvalQuestionType::SingleSessionPreference),
        "SingleSessionPreference"
    );
    assert_eq!(
        format!("{:?}", LongMemEvalQuestionType::MultiSession),
        "MultiSession"
    );
    assert_eq!(
        format!("{:?}", LongMemEvalQuestionType::KnowledgeUpdate),
        "KnowledgeUpdate"
    );
    assert_eq!(
        format!("{:?}", LongMemEvalQuestionType::TemporalReasoning),
        "TemporalReasoning"
    );
    assert_eq!(
        format!("{:?}", LongMemEvalQuestionType::Abstention),
        "Abstention"
    );
}

#[tokio::test]
async fn test_empty_and_erroring_eval_runs() {
    // Empty cases for LoCoMo
    let empty_locomo: Vec<memfuse_bench::locomo::LocomoCase> = vec![];
    let mock_search_ok = |_q: &str| {
        Box::pin(async { Ok(vec![]) })
            as memfuse_bench::long_mem_eval::BoxFuture<
                'static,
                memfuse_core::Result<Vec<ScoredChunk>>,
            >
    };
    let locomo_report = run_locomo_eval(&empty_locomo, mock_search_ok)
        .await
        .unwrap();
    assert_eq!(locomo_report.total_eval_cases, 0);
    assert_eq!(locomo_report.overall_recall_at_5, 0.0);
    assert_eq!(locomo_report.overall_mrr, 0.0);

    // Empty cases for LongMemEval
    let empty_lme: Vec<memfuse_bench::long_mem_eval::LongMemEvalCase> = vec![];
    let lme_report = run_long_mem_eval(&empty_lme, mock_search_ok).await.unwrap();
    assert_eq!(lme_report.total_cases, 0);
    assert_eq!(lme_report.overall_accuracy, 0.0);

    // Erroring search closure
    let mock_search_err = |_q: &str| {
        Box::pin(async {
            Err(memfuse_core::MemFuseError::InvalidInput(
                "Search failed".into(),
            ))
        })
            as memfuse_bench::long_mem_eval::BoxFuture<
                'static,
                memfuse_core::Result<Vec<ScoredChunk>>,
            >
    };
    let dummy_case = memfuse_bench::long_mem_eval::LongMemEvalCase {
        question_id: "q1".into(),
        session_history: vec![],
        question: "Who?".into(),
        answer: serde_json::json!("Alice"),
        question_type: LongMemEvalQuestionType::SingleSessionUser,
    };
    let err_res = run_long_mem_eval(&[dummy_case], mock_search_err).await;
    assert!(err_res.is_err());
    assert!(err_res.unwrap_err().to_string().contains("Search failed"));
}

#[test]
fn test_load_locomo_dataset_varied_json_formats() {
    let temp_dir = tempfile::tempdir().unwrap();

    // 1. Array answer and number answer
    let json_content = r#"[
        {
            "sample_id": "s1",
            "qa": [
                {
                    "question": "Where was Bob?",
                    "answer": ["Paris", "France"],
                    "evidence": ["Bob was in Paris."],
                    "category": 1
                },
                {
                    "question": "How many items?",
                    "answer": 42,
                    "evidence": ["There were 42 items."],
                    "category": 4
                },
                {
                    "question": "Adversarial question?",
                    "adversarial_answer": "No answer possible",
                    "evidence": [],
                    "category": 5
                }
            ]
        }
    ]"#;
    let json_path = temp_dir.path().join("locomo_varied.json");
    std::fs::write(&json_path, json_content).unwrap();

    let cases = load_locomo_dataset(&json_path).unwrap();
    assert_eq!(cases.len(), 3);
    assert_eq!(cases[0].expected_answer, "Paris France");
    assert_eq!(cases[1].expected_answer, "42");
    assert_eq!(cases[2].expected_answer, "No answer possible");

    // 2. Empty JSON array -> InvalidInput
    let empty_json_path = temp_dir.path().join("locomo_empty.json");
    std::fs::write(&empty_json_path, "[]").unwrap();
    let empty_res = load_locomo_dataset(&empty_json_path);
    assert!(empty_res.is_err());
    assert!(empty_res
        .unwrap_err()
        .to_string()
        .contains("No valid LoCoMo cases extracted"));
}

#[test]
fn test_check_regression_gate_thresholds() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let baseline_path = temp_dir.path().join("baseline.json");

    let baseline_report = RegressionReport {
        recall_at_5: 0.95,
        recall_at_10: 0.98,
        overall_accuracy: 0.95,
        total_scenarios: 30,
        failed_scenarios: vec![],
    };

    let json_str = serde_json::to_string_pretty(&baseline_report)?;
    std::fs::write(&baseline_path, json_str)?;

    // Case 1: Identical score -> OK
    assert!(check_regression(&baseline_report, &baseline_path).is_ok());

    // Case 2: Drop <= 3pp (0.95 -> 0.93, delta = 0.02) -> OK
    let minor_drop = RegressionReport {
        recall_at_5: 0.93,
        recall_at_10: 0.98,
        overall_accuracy: 0.93,
        total_scenarios: 30,
        failed_scenarios: vec!["scen_1".into()],
    };
    assert!(check_regression(&minor_drop, &baseline_path).is_ok());

    // Case 3: Drop > 3pp (0.95 -> 0.90, delta = 0.05) -> Err
    let major_drop = RegressionReport {
        recall_at_5: 0.90,
        recall_at_10: 0.95,
        overall_accuracy: 0.90,
        total_scenarios: 30,
        failed_scenarios: vec!["scen_1".into(), "scen_2".into()],
    };
    let res = check_regression(&major_drop, &baseline_path);
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("Recall@5-Regression"));
    assert!(err_msg.contains("0.950 -> 0.900"));

    Ok(())
}
