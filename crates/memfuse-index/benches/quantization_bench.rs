use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_index::quantize::ScalarQuantizer;
use memfuse_core::DistanceMetric;
use rand::Rng;

fn bench_quantization(c: &mut Criterion) {
    let dim = 1536;
    let mut rng = rand::thread_rng();
    let v1: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
    let v2: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();

    let q = ScalarQuantizer::train(&[&v1, &v2], dim);
    let q1 = q.quantize(&v1);
    let q2 = q.quantize(&v2);

    let mut group = c.benchmark_group("Quantization");

    group.bench_function("quantize", |b| {
        b.iter(|| q.quantize(black_box(&v1)))
    });

    group.bench_function("dequantize", |b| {
        b.iter(|| q.dequantize(black_box(&q1)))
    });

    group.bench_function("asymmetric_dist_cosine", |b| {
        b.iter(|| q.asymmetric_dist(black_box(&v1), black_box(&q2), DistanceMetric::Cosine))
    });

    group.bench_function("symmetric_dist_cosine", |b| {
        b.iter(|| q.symmetric_dist(black_box(&q1), black_box(&q2), DistanceMetric::Cosine))
    });

    group.finish();
}

criterion_group!(benches, bench_quantization);
criterion_main!(benches);
