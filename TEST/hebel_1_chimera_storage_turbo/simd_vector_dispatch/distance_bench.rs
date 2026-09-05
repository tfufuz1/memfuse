use chimera_core::DistanceMetric;
use chimera_index_vector::distance::{
    compute_distance, cosine_distance_scalar, dot_product_scalar, euclidean_distance_scalar,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::Rng;

fn generate_vector(dim: usize) -> Vec<f32> {
    let mut rng = rand::thread_rng();
    (0..dim).map(|_| rng.gen::<f32>()).collect()
}

fn bench_distances(c: &mut Criterion) {
    let dim = 1536; // OpenAI embedding size
    let v1 = generate_vector(dim);
    let v2 = generate_vector(dim);

    let mut group = c.benchmark_group("Vector Distances (1536d)");

    // Cosine
    group.bench_with_input(
        BenchmarkId::new("Cosine", "Scalar"),
        &(&v1, &v2),
        |b, (x, y)| b.iter(|| cosine_distance_scalar(black_box(x), black_box(y))),
    );
    group.bench_with_input(
        BenchmarkId::new("Cosine", "SIMD"),
        &(&v1, &v2),
        |b, (x, y)| b.iter(|| compute_distance(black_box(x), black_box(y), DistanceMetric::Cosine)),
    );

    // Euclidean
    group.bench_with_input(
        BenchmarkId::new("Euclidean", "Scalar"),
        &(&v1, &v2),
        |b, (x, y)| b.iter(|| euclidean_distance_scalar(black_box(x), black_box(y))),
    );
    group.bench_with_input(
        BenchmarkId::new("Euclidean", "SIMD"),
        &(&v1, &v2),
        |b, (x, y)| {
            b.iter(|| compute_distance(black_box(x), black_box(y), DistanceMetric::Euclidean))
        },
    );

    // Dot Product
    group.bench_with_input(
        BenchmarkId::new("Dot Product", "Scalar"),
        &(&v1, &v2),
        |b, (x, y)| b.iter(|| dot_product_scalar(black_box(x), black_box(y))),
    );
    group.bench_with_input(
        BenchmarkId::new("Dot Product", "SIMD"),
        &(&v1, &v2),
        |b, (x, y)| {
            b.iter(|| compute_distance(black_box(x), black_box(y), DistanceMetric::DotProduct))
        },
    );

    group.finish();
}

criterion_group!(benches, bench_distances);
criterion_main!(benches);
