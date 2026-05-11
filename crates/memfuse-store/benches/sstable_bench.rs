use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_store::sstable::*;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_sstable_get(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("bench.sst");
    let bc = create_block_cache(16);

    rt.block_on(async {
        let mut builder = SstableBuilder::create(&path).await.unwrap();
        for i in 0..1000 {
            let key = format!("key{:04}", i);
            let val = format!("val{:04}", i);
            builder.add(key.as_bytes(), val.as_bytes(), i as u64).await.unwrap();
        }
        builder.finish().await.unwrap();
    });

    let reader = rt.block_on(async {
        SstableReader::open(&path, bc).await.unwrap()
    });

    let mut group = c.benchmark_group("SSTable");
    group.bench_function("get", |b| {
        b.iter(|| {
            rt.block_on(async {
                let res = reader.get(black_box(b"key0500")).await.unwrap();
                black_box(res);
            })
        })
    });
    group.finish();
}

criterion_group!(benches, bench_sstable_get);
criterion_main!(benches);
