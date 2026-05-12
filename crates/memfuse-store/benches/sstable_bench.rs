use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_store::sstable::*;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_sstable_get(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("bench.sst");
    let bc = create_block_cache(10); // 10MB

    // Setup: Create an SSTable with 1000 entries
    rt.block_on(async {
        let mut builder = SstableBuilder::create(&path).await.unwrap();
        for i in 0..1000 {
            let key = format!("key{:05}", i);
            let value = format!("value{:05}", i);
            builder.add(key.as_bytes(), value.as_bytes(), i as u64).await.unwrap();
        }
        builder.finish().await.unwrap();
    });

    let reader = rt.block_on(async {
        SstableReader::open(&path, bc).await.unwrap()
    });

    let mut group = c.benchmark_group("SstableReader");

    group.bench_function("get_hit_cached", |b| {
        // Pre-warm cache
        rt.block_on(async {
            reader.get(b"key00500").await.unwrap();
        });

        b.to_async(&rt).iter(|| async {
            black_box(reader.get(black_box(b"key00500")).await.unwrap());
        });
    });

    group.bench_function("get_miss", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(reader.get(black_box(b"nonexistent")).await.unwrap());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_sstable_get);
criterion_main!(benches);
