// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Tests für LongMemEval und LoCoMo Benchmark-Module (Parsing, Mock Search Metrics, Missing Dataset Errors)

use memfuse_bench::locomo::{load_locomo_dataset, run_locomo_eval, LocomoQuestionCategory};
use memfuse_bench::long_mem_eval::{
    load_from_jsonl, run_long_mem_eval, LongMemEvalQuestionType, ScoredChunk,
};
use std::path::Path;

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
        }) as memfuse_bench::long_mem_eval::BoxFuture<'static, memfuse_core::Result<Vec<ScoredChunk>>>
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
        }) as memfuse_bench::long_mem_eval::BoxFuture<'static, memfuse_core::Result<Vec<ScoredChunk>>>
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
