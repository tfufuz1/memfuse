use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::DistanceMetric;
use memfuse_index::quantize::ScalarQuantizer;
use rand::Rng;

fn bench_quantization(c: &mut Criterion) {
    let dim = 1536;
    let mut rng = rand::thread_rng();
    let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
    let q = ScalarQuantizer::train(&[&v], dim);
    let qv = q.quantize(&v);
    let norm = q.compute_f32_norm(&v);

    c.bench_function("quantize", |b| b.iter(|| q.quantize(black_box(&v))));

    c.bench_function("asymmetric_dist", |b| {
        b.iter(|| {
            q.asymmetric_dist(
                black_box(&v),
                black_box(&qv),
                DistanceMetric::Cosine,
                black_box(None),
            )
        })
    });

    c.bench_function("asymmetric_dist_cached_norm", |b| {
        b.iter(|| {
            q.asymmetric_dist(
                black_box(&v),
                black_box(&qv),
                DistanceMetric::Cosine,
                black_box(Some(norm)),
            )
        })
    });

    c.bench_function("symmetric_dist", |b| {
        b.iter(|| q.symmetric_dist(black_box(&qv), black_box(&qv), DistanceMetric::Cosine))
    });
}

criterion_group!(benches, bench_quantization);
criterion_main!(benches);
