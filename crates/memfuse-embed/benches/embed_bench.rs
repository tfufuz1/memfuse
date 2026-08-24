use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_embed::TextEmbedder;
use std::path::PathBuf;
use tokio::runtime::Runtime;

fn bench_embed(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut model_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    model_dir.push("tests/data");

    if !model_dir.join("model.onnx").exists() || !model_dir.join("tokenizer.json").exists() {
        println!("Skipping benchmark because ONNX model or tokenizer is missing in tests/data");
        return;
    }

    let embedder = TextEmbedder::load(&model_dir).expect("Failed to load TextEmbedder");

    c.bench_function("embed_async_100_calls", |b| {
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..100 {
                    let _ = embedder
                        .embed_async("This is a benchmark test sentence for ONNX embeddings.")
                        .await
                        .unwrap();
                }
            })
        })
    });
}

criterion_group!(benches, bench_embed);
criterion_main!(benches);
