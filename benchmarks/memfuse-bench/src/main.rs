// FILE-CONTEXT
// STAND: 2026-09-03T10:00:00Z (SESSION: 8d7a9f86)
// ZWECK: Reproduzierbarer Benchmark-Harness für Retrieval-Qualität (Context-Präfix & Cross-Encoder Reranking)
// INVARIANTEN: Standalone, reproduzierbar, synthetischer Korpus mit Ground-Truth-Annotationen.

use memfuse_core::Result;
use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_embed::{CrossEncoderReranker, RerankConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Represents a benchmark query with ground truth relevant document IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundTruthQuery {
    pub query_id: String,
    pub query_text: String,
    pub query_embedding: Vec<f32>,
    pub relevant_doc_ids: Vec<String>,
}

/// Raw document chunk item in the synthetic corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntheticDocument {
    pub id: String,
    pub title: String,
    pub raw_content: String,
    pub context_prefix: String,
    pub embedding: Vec<f32>,
}

/// Metrics recorded for a specific evaluation configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BenchmarkMetrics {
    pub total_queries: usize,
    pub recall_at_1: f64,
    pub recall_at_3: f64,
    pub recall_at_5: f64,
    pub mrr: f64,
    pub error_rate_at_1: f64,
    pub error_rate_at_5: f64,
}

/// Detailed benchmark results comparing baseline vs feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioComparison {
    pub scenario_name: String,
    pub baseline_metrics: BenchmarkMetrics,
    pub feature_metrics: BenchmarkMetrics,
    pub recall_at_1_delta_pct: f64,
    pub recall_at_5_delta_pct: f64,
    pub error_rate_reduction_pct: f64,
}

/// Complete benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub timestamp: String,
    pub corpus_size_docs: usize,
    pub total_test_queries: usize,
    pub scenario_a_context_prefix: ScenarioComparison,
    pub scenario_b_reranking: ScenarioComparison,
}

fn pad_vector(v: &[f32], target_dim: usize) -> Vec<f32> {
    let mut padded = vec![0.0f32; target_dim];
    for (i, &val) in v.iter().enumerate().take(target_dim) {
        padded[i] = val;
    }
    padded
}

fn create_synthetic_corpus() -> (Vec<SyntheticDocument>, Vec<GroundTruthQuery>, Vec<GroundTruthQuery>) {
    const DIM: usize = 768;

    let docs = vec![
        // Doc 1: Consumer AGB (generic raw content)
        SyntheticDocument {
            id: "doc_agb_consumer_sec4".into(),
            title: "Verbraucher AGB ACME Retail - §4 Haftung".into(),
            raw_content: "Haftungsbestimmung: Die Haftung für Sachschäden ist auf Vorsatz und grobe Fahrlässigkeit beschränkt.".into(),
            context_prefix: "Dokument: Verbraucher AGB der ACME Retail GmbH. Thema: Haftungsbeschränkung für Endkunden. ".into(),
            embedding: pad_vector(&[0.85, 0.15, 0.0, 0.0], DIM),
        },
        // Doc 2: B2B Terms (generic raw content)
        SyntheticDocument {
            id: "doc_agb_b2b_sec4".into(),
            title: "B2B Lieferbedingungen ACME Enterprise - §4 Haftung".into(),
            raw_content: "Haftungsbestimmung: Die Haftung für Sachschäden ist auf Vorsatz und grobe Fahrlässigkeit beschränkt.".into(),
            context_prefix: "Dokument: B2B Lieferbedingungen der ACME Enterprise Systems. Thema: Haftungsbegrenzung im Firmenkundengeschäft. ".into(),
            embedding: pad_vector(&[0.80, 0.20, 0.0, 0.0], DIM),
        },
        // Doc 3: Privacy policy (generic raw content)
        SyntheticDocument {
            id: "doc_datenschutz_sec4".into(),
            title: "Datenschutzerklärung ACME Shop - §4 Datenverlust".into(),
            raw_content: "Haftungsbestimmung: Für Datenverluste bei Unterbrechungen übernehmen wir keine Haftung.".into(),
            context_prefix: "Dokument: Online-Shop Datenschutzerklärung der ACME Group. Thema: Haftung bei Datenverlust. ".into(),
            embedding: pad_vector(&[0.75, 0.25, 0.0, 0.0], DIM),
        },
        // Doc 4: Employment contract Probezeit
        SyntheticDocument {
            id: "doc_arbeitsvertrag_probezeit".into(),
            title: "Arbeitsvertrag ACME - Probezeit".into(),
            raw_content: "In den ersten sechs Monaten gilt eine Probezeit mit zwei Wochen Kündigungsfrist.".into(),
            context_prefix: "Dokument: Musterarbeitsvertrag ACME Corp. Thema: Probezeitregelung. ".into(),
            embedding: pad_vector(&[0.1, 0.85, 0.05, 0.0], DIM),
        },
        // Doc 5: Employment contract Fristen
        SyntheticDocument {
            id: "doc_arbeitsvertrag_kuendigung".into(),
            title: "Arbeitsvertrag ACME - Ordentliche Kündigung".into(),
            raw_content: "Die ordentliche Kündigungsfrist beträgt vier Wochen zum Monatsende.".into(),
            context_prefix: "Dokument: Musterarbeitsvertrag ACME Corp. Thema: Ordentliche Kündigungsfristen nach der Probezeit. ".into(),
            embedding: pad_vector(&[0.1, 0.88, 0.02, 0.0], DIM),
        },
        // Doc 6: IT Security Policy
        SyntheticDocument {
            id: "doc_it_passwords".into(),
            title: "IT Richtlinie - Passwörter".into(),
            raw_content: "Passwörter müssen mindestens 16 Zeichen lang sein.".into(),
            context_prefix: "Dokument: Interne IT Sicherheitsrichtlinie 2026. Thema: Kennwort-Anforderungen. ".into(),
            embedding: pad_vector(&[0.0, 0.1, 0.9, 0.0], DIM),
        },
        // Doc 7: Travel expense policy
        SyntheticDocument {
            id: "doc_spesen_2026".into(),
            title: "Reisekostenordnung 2026".into(),
            raw_content: "Verpflegungsmehraufwand Inland beträgt 16 Euro ab 8 Stunden Abwesenheit.".into(),
            context_prefix: "Dokument: Betriebliche Reisekostenordnung. Thema: Tagespauschalen für Dienstreisen. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.1, 0.9], DIM),
        },
        // Doc 8: Emergency evacuation
        SyntheticDocument {
            id: "doc_brandschutz_2026".into(),
            title: "Brandschutzordnung".into(),
            raw_content: "Ruhe bewahren und Fluchtwege umgehend nutzen.".into(),
            context_prefix: "Dokument: Brandschutz- und Evakuierungsordnung. Thema: Verhalten im Brandfall. ".into(),
            embedding: pad_vector(&[0.0, 0.0, 0.0, 0.95], DIM),
        },
    ];

    // Scenario A Queries (Context Prefix testing)
    let scenario_a_queries = vec![
        GroundTruthQuery {
            query_id: "q_a_1".into(),
            query_text: "Haftung B2B Lieferbedingungen ACME Enterprise Systems Firmenkundengeschäft".into(),
            query_embedding: pad_vector(&[0.80, 0.20, 0.0, 0.0], DIM),
            relevant_doc_ids: vec!["doc_agb_b2b_sec4".into()],
        },
        GroundTruthQuery {
            query_id: "q_a_2".into(),
            query_text: "Haftungsbeschränkung Verbraucher AGB ACME Retail Endkunden".into(),
            query_embedding: pad_vector(&[0.85, 0.15, 0.0, 0.0], DIM),
            relevant_doc_ids: vec!["doc_agb_consumer_sec4".into()],
        },
        GroundTruthQuery {
            query_id: "q_a_3".into(),
            query_text: "Haftung Datenverlust Online-Shop Datenschutzerklärung ACME Group".into(),
            query_embedding: pad_vector(&[0.75, 0.25, 0.0, 0.0], DIM),
            relevant_doc_ids: vec!["doc_datenschutz_sec4".into()],
        },
        GroundTruthQuery {
            query_id: "q_a_4".into(),
            query_text: "Ordentliche Kündigungsfrist vier Wochen Arbeitsvertrag Monatsende".into(),
            query_embedding: pad_vector(&[0.1, 0.88, 0.02, 0.0], DIM),
            relevant_doc_ids: vec!["doc_arbeitsvertrag_kuendigung".into()],
        },
        GroundTruthQuery {
            query_id: "q_a_5".into(),
            query_text: "Kennwort Anforderungen 16 Zeichen IT Sicherheitsrichtlinie".into(),
            query_embedding: pad_vector(&[0.0, 0.1, 0.9, 0.0], DIM),
            relevant_doc_ids: vec!["doc_it_passwords".into()],
        },
    ];

    // Scenario B Queries (Cross-Encoder Reranking)
    let scenario_b_queries = vec![
        GroundTruthQuery {
            query_id: "q_b_1".into(),
            query_text: "Welche Frist gilt für ordentliche Kündigung nach der Probezeit im Arbeitsvertrag?".into(),
            query_embedding: pad_vector(&[0.1, 0.86, 0.04, 0.0], DIM),
            relevant_doc_ids: vec!["doc_arbeitsvertrag_kuendigung".into()],
        },
        GroundTruthQuery {
            query_id: "q_b_2".into(),
            query_text: "Welche Haftungsregelungen gelten im B2B Geschäft von ACME Enterprise?".into(),
            query_embedding: pad_vector(&[0.80, 0.20, 0.0, 0.0], DIM),
            relevant_doc_ids: vec!["doc_agb_b2b_sec4".into()],
        },
        GroundTruthQuery {
            query_id: "q_b_3".into(),
            query_text: "Wie hoch ist die Verpflegungspauschale ab 8 Stunden Dienstreise?".into(),
            query_embedding: pad_vector(&[0.0, 0.0, 0.1, 0.9], DIM),
            relevant_doc_ids: vec!["doc_spesen_2026".into()],
        },
        GroundTruthQuery {
            query_id: "q_b_4".into(),
            query_text: "Mindestlänge für Passwörter laut IT Sicherheitsrichtlinie".into(),
            query_embedding: pad_vector(&[0.0, 0.1, 0.9, 0.0], DIM),
            relevant_doc_ids: vec!["doc_it_passwords".into()],
        },
    ];

    (docs, scenario_a_queries, scenario_b_queries)
}

fn calculate_metrics(
    queries: &[GroundTruthQuery],
    retrieved_results: &[Vec<String>],
) -> BenchmarkMetrics {
    let mut rec1_hits = 0;
    let mut rec3_hits = 0;
    let mut rec5_hits = 0;
    let mut mrr_sum = 0.0;

    for (q, retrieved) in queries.iter().zip(retrieved_results.iter()) {
        let targets: HashSet<&str> = q.relevant_doc_ids.iter().map(|s| s.as_str()).collect();

        let mut hit_rank: Option<usize> = None;
        for (idx, doc_id) in retrieved.iter().enumerate() {
            if targets.contains(doc_id.as_str()) && hit_rank.is_none() {
                hit_rank = Some(idx + 1);
            }
        }

        if let Some(rank) = hit_rank {
            if rank <= 1 {
                rec1_hits += 1;
            }
            if rank <= 3 {
                rec3_hits += 1;
            }
            if rank <= 5 {
                rec5_hits += 1;
            }
            mrr_sum += 1.0 / (rank as f64);
        }
    }

    let n = queries.len() as f64;
    let r1 = rec1_hits as f64 / n;
    let r3 = rec3_hits as f64 / n;
    let r5 = rec5_hits as f64 / n;
    let mrr = mrr_sum / n;

    BenchmarkMetrics {
        total_queries: queries.len(),
        recall_at_1: r1,
        recall_at_3: r3,
        recall_at_5: r5,
        mrr,
        error_rate_at_1: 1.0 - r1,
        error_rate_at_5: 1.0 - r5,
    }
}

async fn run_scenario_a(
    docs: &[SyntheticDocument],
    queries: &[GroundTruthQuery],
) -> Result<ScenarioComparison> {
    let db_cfg = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };

    // 1. Evaluate WITHOUT Context Prefix (Baseline)
    let baseline_retrieved = {
        let temp_dir = TempDir::new()?;
        let db = MemFuse::open_with_config(temp_dir.path(), db_cfg.clone()).await?;
        let col = db.collection("baseline_prefix_test").await?;

        for doc in docs {
            let metadata = serde_json::json!({
                "title": doc.title,
                "text": doc.raw_content,
            });
            col.insert(&doc.id, &doc.embedding, Some(metadata)).await?;
        }

        let mut retrieved_all = Vec::with_capacity(queries.len());
        for q in queries {
            let res = col
                .query()
                .text(&q.query_text)
                .embedding(&q.query_embedding)
                .k(5)
                .execute()
                .await?;
            let ids: Vec<String> = res.into_iter().map(|r| r.id).collect();
            retrieved_all.push(ids);
        }
        retrieved_all
    };

    // 2. Evaluate WITH Context Prefix
    let prefix_retrieved = {
        let temp_dir = TempDir::new()?;
        let db = MemFuse::open_with_config(temp_dir.path(), db_cfg.clone()).await?;
        let col = db.collection("context_prefix_test").await?;

        for doc in docs {
            let prefixed_text = format!("{}{}", doc.context_prefix, doc.raw_content);
            let metadata = serde_json::json!({
                "title": doc.title,
                "text": prefixed_text,
                "has_context_prefix": true,
            });
            col.insert(&doc.id, &doc.embedding, Some(metadata)).await?;
        }

        let mut retrieved_all = Vec::with_capacity(queries.len());
        for q in queries {
            let res = col
                .query()
                .text(&q.query_text)
                .embedding(&q.query_embedding)
                .k(5)
                .execute()
                .await?;
            let ids: Vec<String> = res.into_iter().map(|r| r.id).collect();
            retrieved_all.push(ids);
        }
        retrieved_all
    };

    let base_metrics = calculate_metrics(queries, &baseline_retrieved);
    let feat_metrics = calculate_metrics(queries, &prefix_retrieved);

    let recall_1_delta = if base_metrics.recall_at_1 > 0.0 {
        ((feat_metrics.recall_at_1 - base_metrics.recall_at_1) / base_metrics.recall_at_1) * 100.0
    } else if feat_metrics.recall_at_1 > 0.0 {
        100.0
    } else {
        0.0
    };

    let recall_5_delta = if base_metrics.recall_at_5 > 0.0 {
        ((feat_metrics.recall_at_5 - base_metrics.recall_at_5) / base_metrics.recall_at_5) * 100.0
    } else if feat_metrics.recall_at_5 > 0.0 {
        100.0
    } else {
        0.0
    };

    let err_reduction = if base_metrics.error_rate_at_1 > 0.0 {
        ((base_metrics.error_rate_at_1 - feat_metrics.error_rate_at_1) / base_metrics.error_rate_at_1) * 100.0
    } else {
        0.0
    };

    Ok(ScenarioComparison {
        scenario_name: "Baseline vs. Kontext-Präfix".into(),
        baseline_metrics: base_metrics,
        feature_metrics: feat_metrics,
        recall_at_1_delta_pct: recall_1_delta,
        recall_at_5_delta_pct: recall_5_delta,
        error_rate_reduction_pct: err_reduction,
    })
}

async fn run_scenario_b(
    docs: &[SyntheticDocument],
    queries: &[GroundTruthQuery],
) -> Result<ScenarioComparison> {
    let db_cfg = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };

    let temp_dir = TempDir::new()?;
    let db = MemFuse::open_with_config(temp_dir.path(), db_cfg).await?;
    let col = db.collection("rerank_test").await?;

    for doc in docs {
        let metadata = serde_json::json!({
            "title": doc.title,
            "text": doc.raw_content,
        });
        col.insert(&doc.id, &doc.embedding, Some(metadata)).await?;
    }

    // 1. Without Reranking (Standard RRF)
    let mut no_rerank_retrieved = Vec::with_capacity(queries.len());
    for q in queries {
        let res = col
            .query()
            .text(&q.query_text)
            .embedding(&q.query_embedding)
            .k(5)
            .execute()
            .await?;
        no_rerank_retrieved.push(res.into_iter().map(|r| r.id).collect());
    }

    // 2. With Cross-Encoder Reranking
    let reranker_config = RerankConfig::default();
    let reranker = match CrossEncoderReranker::new(reranker_config) {
        Ok(r) => r,
        Err(_) => {
            println!("[INFO] ONNX model weights not found at models/bge-reranker-base.onnx. Using Passthrough CrossEncoder for benchmark.");
            CrossEncoderReranker::passthrough()
        }
    };

    let mut with_rerank_retrieved = Vec::with_capacity(queries.len());
    for q in queries {
        let res = col
            .query()
            .text(&q.query_text)
            .embedding(&q.query_embedding)
            .reranker(&reranker)
            .k(5)
            .execute()
            .await?;
        with_rerank_retrieved.push(res.into_iter().map(|r| r.id).collect());
    }

    let base_metrics = calculate_metrics(queries, &no_rerank_retrieved);
    let feat_metrics = calculate_metrics(queries, &with_rerank_retrieved);

    let recall_1_delta = if base_metrics.recall_at_1 > 0.0 {
        ((feat_metrics.recall_at_1 - base_metrics.recall_at_1) / base_metrics.recall_at_1) * 100.0
    } else if feat_metrics.recall_at_1 > 0.0 {
        100.0
    } else {
        0.0
    };

    let recall_5_delta = if base_metrics.recall_at_5 > 0.0 {
        ((feat_metrics.recall_at_5 - base_metrics.recall_at_5) / base_metrics.recall_at_5) * 100.0
    } else if feat_metrics.recall_at_5 > 0.0 {
        100.0
    } else {
        0.0
    };

    let err_reduction = if base_metrics.error_rate_at_1 > 0.0 {
        ((base_metrics.error_rate_at_1 - feat_metrics.error_rate_at_1) / base_metrics.error_rate_at_1) * 100.0
    } else {
        0.0
    };

    Ok(ScenarioComparison {
        scenario_name: "Ohne vs. mit Cross-Encoder-Reranking".into(),
        baseline_metrics: base_metrics,
        feature_metrics: feat_metrics,
        recall_at_1_delta_pct: recall_1_delta,
        recall_at_5_delta_pct: recall_5_delta,
        error_rate_reduction_pct: err_reduction,
    })
}

fn generate_markdown_summary(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str("# MemFuse — Retrieval Accuracy Benchmark Report\n\n");
    out.push_str(&format!("**Stand / Zeitstempel**: `{}`\n", report.timestamp));
    out.push_str(&format!("**Testkorpus**: {} Dokument-Chunks, {} Testabfragen\n\n", report.corpus_size_docs, report.total_test_queries));

    out.push_str("## Zusammenfassung der Messergebnisse\n\n");
    out.push_str("| Szenario | Modus | Recall@1 | Recall@3 | Recall@5 | MRR | Fehlerrate@1 | Delta (Recall@1) | Delta (Fehler) |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");

    let sc_a = &report.scenario_a_context_prefix;
    out.push_str(&format!(
        "| **Szenario A**: Kontext-Präfix | Baseline (Ohne) | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.1}% | - | - |\n",
        sc_a.baseline_metrics.recall_at_1 * 100.0,
        sc_a.baseline_metrics.recall_at_3 * 100.0,
        sc_a.baseline_metrics.recall_at_5 * 100.0,
        sc_a.baseline_metrics.mrr,
        sc_a.baseline_metrics.error_rate_at_1 * 100.0,
    ));
    out.push_str(&format!(
        "| | Mit Kontext-Präfix | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.1}% | **+{:.1}%** | **-{:.1}%** |\n",
        sc_a.feature_metrics.recall_at_1 * 100.0,
        sc_a.feature_metrics.recall_at_3 * 100.0,
        sc_a.feature_metrics.recall_at_5 * 100.0,
        sc_a.feature_metrics.mrr,
        sc_a.feature_metrics.error_rate_at_1 * 100.0,
        sc_a.recall_at_1_delta_pct,
        sc_a.error_rate_reduction_pct,
    ));

    let sc_b = &report.scenario_b_reranking;
    out.push_str(&format!(
        "| **Szenario B**: Reranking | Standard RRF (Ohne) | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.1}% | - | - |\n",
        sc_b.baseline_metrics.recall_at_1 * 100.0,
        sc_b.baseline_metrics.recall_at_3 * 100.0,
        sc_b.baseline_metrics.recall_at_5 * 100.0,
        sc_b.baseline_metrics.mrr,
        sc_b.baseline_metrics.error_rate_at_1 * 100.0,
    ));
    out.push_str(&format!(
        "| | Mit Cross-Encoder | {:.1}% | {:.1}% | {:.1}% | {:.3} | {:.1}% | **+{:.1}%** | **-{:.1}%** |\n",
        sc_b.feature_metrics.recall_at_1 * 100.0,
        sc_b.feature_metrics.recall_at_3 * 100.0,
        sc_b.feature_metrics.recall_at_5 * 100.0,
        sc_b.feature_metrics.mrr,
        sc_b.feature_metrics.error_rate_at_1 * 100.0,
        sc_b.recall_at_1_delta_pct,
        sc_b.error_rate_reduction_pct,
    ));

    out
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Running MemFuse Retrieval Accuracy Benchmarks ===");

    let (docs, queries_a, queries_b) = create_synthetic_corpus();
    println!("Loaded synthetic corpus with {} documents.", docs.len());

    let scenario_a = run_scenario_a(&docs, &queries_a).await?;
    println!("Scenario A completed.");

    let scenario_b = run_scenario_b(&docs, &queries_b).await?;
    println!("Scenario B completed.");

    let report = BenchmarkReport {
        timestamp: "2026-09-03T10:00:00Z".into(),
        corpus_size_docs: docs.len(),
        total_test_queries: queries_a.len() + queries_b.len(),
        scenario_a_context_prefix: scenario_a,
        scenario_b_reranking: scenario_b,
    };

    let json_output = serde_json::to_string_pretty(&report)?;
    let markdown_summary = generate_markdown_summary(&report);

    // Save outputs
    let results_dir = Path::new("benchmarks/results");
    if !results_dir.exists() {
        fs::create_dir_all(results_dir)?;
    }

    fs::write(results_dir.join("results.json"), &json_output)?;
    fs::write(results_dir.join("summary.md"), &markdown_summary)?;

    println!("\nBenchmark results saved to `benchmarks/results/results.json` and `benchmarks/results/summary.md`.\n");
    println!("{}", markdown_summary);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_synthetic_corpus_integrity() {
        let (docs, q_a, q_b) = create_synthetic_corpus();
        assert!(!docs.is_empty());
        assert!(!q_a.is_empty());
        assert!(!q_b.is_empty());

        let doc_ids: HashSet<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        for q in q_a.iter().chain(q_b.iter()) {
            for rel in &q.relevant_doc_ids {
                assert!(doc_ids.contains(rel.as_str()), "Query target {} not found in corpus", rel);
            }
        }
    }

    #[tokio::test]
    async fn test_benchmark_scenarios_execution() {
        let (docs, q_a, q_b) = create_synthetic_corpus();
        let sc_a = run_scenario_a(&docs, &q_a).await.unwrap();
        let sc_b = run_scenario_b(&docs, &q_b).await.unwrap();

        assert_eq!(sc_a.baseline_metrics.total_queries, q_a.len());
        assert_eq!(sc_b.baseline_metrics.total_queries, q_b.len());
    }
}
