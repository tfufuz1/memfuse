use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::DistanceMetric;
use memfuse_index::quantize::ScalarQuantizer;
use rand::Rng;

fn bench_sq8(c: &mut Criterion) {
    let dim = 1536;
    let mut rng = rand::thread_rng();

    // Generate batch for training
    let mut batch = Vec::new();
    for _ in 0..10 {
        let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        batch.push(v);
    }
    let refs: Vec<&[f32]> = batch.iter().map(|v| v.as_slice()).collect();
    let quantizer = ScalarQuantizer::train(&refs, dim);

    let a: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
    let b: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();

    let a_q = quantizer.quantize(&a);
    let b_q = quantizer.quantize(&b);

    let mut group = c.benchmark_group("SQ8");

    group.bench_function("quantize", |b_bench| {
        b_bench.iter(|| quantizer.quantize(black_box(&a)))
    });

    group.bench_function("dequantize", |b_bench| {
        b_bench.iter(|| quantizer.dequantize(black_box(&a_q)))
    });

    group.bench_function("asymmetric_dist_cosine", |b_bench| {
        b_bench.iter(|| {
            quantizer.asymmetric_dist(black_box(&a), black_box(&b_q), DistanceMetric::Cosine)
        })
    });

    group.bench_function("symmetric_dist_cosine", |b_bench| {
        b_bench.iter(|| {
            quantizer.symmetric_dist(black_box(&a_q), black_box(&b_q), DistanceMetric::Cosine)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_sq8);
criterion_main!(benches);
