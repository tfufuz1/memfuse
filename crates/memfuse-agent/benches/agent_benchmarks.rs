//! Performance Benchmark Suite for `memfuse-agent`.
//!
//! Measures loop cycle latency, isolated audit log write overhead,
//! parallel throughput scaling, and transition latency vs history length.

use memfuse_agent::audit::{AuditEntry, AuditLog};
use memfuse_agent::step::StepResult;
use memfuse_agent::{AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

struct BenchTool;

#[async_trait::async_trait]
impl AgentTool for BenchTool {
    fn name(&self) -> &str {
        "bench_tool"
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        Ok(StepResult {
            node_id: "bench_tool".to_string(),
            output: json!({"status": "ok"}),
            tokens_consumed: 1,
            next_edge: None,
        })
    }
}

#[tokio::main]
async fn main() {
    println!("===========================================================");
    println!("         MEMFUSE-AGENT BENCHMARK SUITE PERFORMANCE REPORT  ");
    println!("===========================================================");

    bench_loop_cycle_latency().await;
    bench_audit_log_write_overhead().await;
    bench_parallel_throughput().await;
    bench_history_scaling().await;

    println!("===========================================================");
}

async fn setup_bench_env() -> (OrchestratorEngine, Arc<MemFuse>, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 50_000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );
    let engine = OrchestratorEngine::from_db(&db);
    (engine, db, tmp)
}

async fn bench_loop_cycle_latency() {
    let (mut engine, db, _tmp) = setup_bench_env().await;
    engine.try_register_tool(Box::new(BenchTool)).unwrap();

    let state_col = db.collection("agent-state").await.unwrap();

    let iterations = 100;
    let mut total_duration = std::time::Duration::ZERO;

    for i in 0..iterations {
        let task_id = format!("bench-loop-{i}");
        let mut ctx = AgentContext::try_new(
            &task_id,
            "start",
            db.clone(),
            state_col.clone(),
            TokenBudget::new(10_000, 0),
        )
        .unwrap();

        let mut g = StateGraph::new();
        g.try_add_node("start", "Start", NodeType::Start, None)
            .unwrap();
        g.try_add_node("step1", "Step 1", NodeType::Task, Some("bench_tool"))
            .unwrap();
        g.try_add_node("end", "End", NodeType::End, None).unwrap();
        g.try_add_edge("start", "step1", None, 1).unwrap();
        g.try_add_edge("step1", "end", None, 1).unwrap();

        let start = Instant::now();
        engine.run(&mut ctx, &g).await.unwrap();
        total_duration += start.elapsed();
    }

    let avg_us = (total_duration.as_micros() as f64) / (iterations as f64);
    println!(
        "[BENCH 1] Average Loop Cycle Latency (checkpoint->execute->commit->audit): {:.2} us ({:.3} ms)",
        avg_us,
        avg_us / 1000.0
    );
}

async fn bench_audit_log_write_overhead() {
    let (_engine, db, _tmp) = setup_bench_env().await;
    let state_col = db.collection("agent-state").await.unwrap();
    let audit_log = AuditLog::new(state_col);

    let iterations = 500;
    let start = Instant::now();

    for i in 0..iterations {
        let entry = AuditEntry {
            task_id: "bench-audit-task".to_string(),
            step_count: i as u64,
            node_id: "step_node".to_string(),
            tokens_consumed: 10,
            payload: json!({"bench": i}),
            error: None,
        };
        audit_log.append(&entry).await.unwrap();
    }

    let elapsed = start.elapsed();
    let avg_us = (elapsed.as_micros() as f64) / (iterations as f64);
    println!(
        "[BENCH 2] Isolated Audit Log Write Latency per entry: {:.2} us ({:.3} ms)",
        avg_us,
        avg_us / 1000.0
    );
}

async fn bench_parallel_throughput() {
    let (_engine, db, _tmp) = setup_bench_env().await;
    let state_col = db.collection("agent-state").await.unwrap();

    let concurrency_levels = vec![5, 20, 50];

    for concurrency in concurrency_levels {
        let start = Instant::now();
        let mut tasks = Vec::new();

        for c in 0..concurrency {
            let task_id = format!("parallel-bench-{concurrency}-{c}");
            let db_c = db.clone();
            let col_c = state_col.clone();

            tasks.push(tokio::spawn(async move {
                let mut eng = OrchestratorEngine::from_db(&db_c);
                eng.try_register_tool(Box::new(BenchTool)).unwrap();

                let mut ctx = AgentContext::try_new(
                    &task_id,
                    "start",
                    db_c,
                    col_c,
                    TokenBudget::new(10_000, 0),
                )
                .unwrap();

                let mut g = StateGraph::new();
                g.try_add_node("start", "Start", NodeType::Start, None)
                    .unwrap();
                g.try_add_node("step1", "Step 1", NodeType::Task, Some("bench_tool"))
                    .unwrap();
                g.try_add_node("end", "End", NodeType::End, None).unwrap();
                g.try_add_edge("start", "step1", None, 1).unwrap();
                g.try_add_edge("step1", "end", None, 1).unwrap();

                eng.run(&mut ctx, &g).await.unwrap();
            }));
        }

        for t in tasks {
            t.await.unwrap();
        }

        let elapsed = start.elapsed();
        let throughput = (concurrency as f64) / elapsed.as_secs_f64();
        println!(
            "[BENCH 3] Concurrency = {:2} | Total Time: {:6.2} ms | Throughput: {:6.2} workflows/sec",
            concurrency,
            elapsed.as_secs_f64() * 1000.0,
            throughput
        );
    }
}

async fn bench_history_scaling() {
    let (mut engine, db, _tmp) = setup_bench_env().await;
    engine.try_register_tool(Box::new(BenchTool)).unwrap();
    let state_col = db.collection("agent-state").await.unwrap();

    let chain_lengths = vec![10, 30, 50];

    for len in chain_lengths {
        let task_id = format!("bench-scale-{len}");
        let mut ctx = AgentContext::try_new(
            &task_id,
            "start",
            db.clone(),
            state_col.clone(),
            TokenBudget::new(100_000, 0),
        )
        .unwrap();

        let mut g = StateGraph::new();
        g.try_add_node("start", "Start", NodeType::Start, None)
            .unwrap();

        let mut prev = "start".to_string();
        for i in 0..len {
            let node_id = format!("step_{i}");
            g.try_add_node(&node_id, "Task", NodeType::Task, Some("bench_tool"))
                .unwrap();
            g.try_add_edge(&prev, &node_id, None, 1).unwrap();
            prev = node_id;
        }

        g.try_add_node("end", "End", NodeType::End, None).unwrap();
        g.try_add_edge(&prev, "end", None, 1).unwrap();

        let start = Instant::now();
        engine.run(&mut ctx, &g).await.unwrap();
        let elapsed = start.elapsed();

        let avg_per_step_us = (elapsed.as_micros() as f64) / (len as f64);
        println!(
            "[BENCH 4] Workflow Chain Length = {:3} steps | Total Time: {:6.2} ms | Avg Latency/Step: {:.2} us",
            len,
            elapsed.as_secs_f64() * 1000.0,
            avg_per_step_us
        );
    }
}
