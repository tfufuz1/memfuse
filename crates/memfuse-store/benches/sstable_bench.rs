use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_store::sstable::{create_block_cache, SstableBuilder, SstableReader};
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_sstable_get(c: &mut Criterion) {
    let rt = Runtime::new().expect("unwrap"); // unwrap
    let tmp = TempDir::new().expect("unwrap"); // unwrap
    let path = tmp.path().join("bench.sst");
    let bc = create_block_cache(16);

    rt.block_on(async {
        let mut builder = SstableBuilder::create(&path).await.expect("unwrap"); // unwrap
        for i in 0..1000 {
            let key = format!("key{:05}", i);
            let val = format!("val{:05}", i);
            builder
                .add(key.as_bytes(), val.as_bytes(), i as u64)
                .await
                .expect("unwrap"); // unwrap
        }
        builder.finish().await.expect("unwrap"); // unwrap
    });

    let reader = rt.block_on(async { SstableReader::open(&path, bc).await.expect("unwrap") }); // unwrap

    let mut group = c.benchmark_group("SSTable");
    group.bench_function("get_existing", |b| {
        b.to_async(&rt).iter(|| async {
            let res = reader.get(black_box(b"key00500")).await.expect("unwrap"); // unwrap
            black_box(res);
        })
    });

    group.bench_function("get_nonexistent", |b| {
        b.to_async(&rt).iter(|| async {
            let res = reader.get(black_box(b"key99999")).await.expect("unwrap"); // unwrap
            black_box(res);
        })
    });
    group.finish();
}

criterion_group!(benches, bench_sstable_get);
criterion_main!(benches);
