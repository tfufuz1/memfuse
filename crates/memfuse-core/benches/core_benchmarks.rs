use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{
    ipc::jsonrpc::JsonRpcRequest,
    snapshot::SnapshotRegistry,
    tx_buffer::{IndexOp, TxBuffer},
    types::{DocId, TxId},
};
use std::sync::Arc;
use std::time::Duration;

fn bench_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("types_throughput");

    group.bench_function("doc_id_from_key", |b| {
        b.iter(|| {
            let id = DocId::from_key(black_box("document_key_1234567890")).unwrap();
            black_box(id);
        });
    });

    group.bench_function("tx_id_creation_and_check", |b| {
        b.iter(|| {
            let tx = TxId::new(black_box(1_000_000));
            black_box(tx.is_valid_origin());
        });
    });

    group.finish();
}

fn bench_tx_buffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("tx_buffer_lifecycle");

    for concurrency in [1, 10, 100, 1000] {
        group.bench_function(format!("stage_commit_reap_concurrent_{}", concurrency), |b| {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let buffer = Arc::new(TxBuffer::<String>::new_with_config(16, Duration::from_millis(100)));

            b.to_async(&runtime).iter(|| {
                let buf = buffer.clone();
                async move {
                    let mut handles = vec![];
                    for i in 0..concurrency {
                        let b = buf.clone();
                        handles.push(tokio::spawn(async move {
                            let tx = TxId::new(1000 + i);
                            b.begin(tx);
                            b.stage(tx, IndexOp::Insert { doc_id: DocId::new(i), data: "bench_data".into() }).unwrap();
                            b.drain(tx);
                        }));
                    }
                    for h in handles {
                        h.await.unwrap();
                    }
                }
            });
        });
    }

    group.finish();
}

fn bench_snapshot_registry(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_registry");

    for snapshot_count in [1, 10, 100, 1000] {
        group.bench_function(format!("pin_unpin_latency_scale_{}", snapshot_count), |b| {
            let registry = Arc::new(SnapshotRegistry::new());
            let mut _guards = vec![];
            for i in 0..snapshot_count {
                _guards.push(registry.register(i as u64));
            }

            b.iter(|| {
                let g = registry.register(black_box(500));
                black_box(registry.min_active_seqno());
                drop(g);
            });
        });
    }

    group.finish();
}

fn bench_ipc_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("ipc_serialization");

    for payload_size_kb in [1, 64, 1024] {
        let text_data = "a".repeat(payload_size_kb * 1024);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "search".into(),
            params: serde_json::json!({ "data": text_data }),
            id: Some(serde_json::json!(1)),
        };

        group.bench_function(format!("jsonrpc_serialize_{}kb", payload_size_kb), |b| {
            b.iter(|| {
                let ser = serde_json::to_string(black_box(&req)).unwrap();
                black_box(ser);
            });
        });

        let serialized = serde_json::to_string(&req).unwrap();
        group.bench_function(format!("jsonrpc_deserialize_{}kb", payload_size_kb), |b| {
            b.iter(|| {
                let deser: JsonRpcRequest = serde_json::from_str(black_box(&serialized)).unwrap();
                black_box(deser);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_types, bench_tx_buffer, bench_snapshot_registry, bench_ipc_serialization);
criterion_main!(benches);
