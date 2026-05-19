use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_index::distance::*;
use rand::Rng;

fn bench_distances(c: &mut Criterion) {
    let dim = 1536;
    let mut rng = rand::thread_rng();
    let a: Vec<f32> = (0..dim).map(|_| rng.gen()).collect();
    let b: Vec<f32> = (0..dim).map(|_| rng.gen()).collect();

    let mut group = c.benchmark_group("Distances");

    group.bench_function("cosine_simd", |b_bench| {
        b_bench.iter(|| cosine_distance(black_box(&a), black_box(&b)))
    });

    group.bench_function("cosine_scalar", |b_bench| {
        b_bench.iter(|| cosine_distance_scalar(black_box(&a), black_box(&b)))
    });

    group.bench_function("euclidean_simd", |b_bench| {
        b_bench.iter(|| euclidean_distance(black_box(&a), black_box(&b)))
    });

    group.bench_function("euclidean_scalar", |b_bench| {
        b_bench.iter(|| euclidean_distance_scalar(black_box(&a), black_box(&b)))
    });

    group.bench_function("dot_product_simd", |b_bench| {
        b_bench.iter(|| dot_product_distance(black_box(&a), black_box(&b)))
    });

    group.bench_function("dot_product_scalar", |b_bench| {
        b_bench.iter(|| dot_product_scalar(black_box(&a), black_box(&b)))
    });

    group.finish();
}

fn bench_u8_distances(c: &mut Criterion) {
    let dim = 1536;
    let mut rng = rand::thread_rng();
    let a: Vec<u8> = (0..dim).map(|_| rng.gen()).collect();
    let b: Vec<u8> = (0..dim).map(|_| rng.gen()).collect();

    let mut group = c.benchmark_group("U8 Distances");

    group.bench_function("dot_product_u8", |b_bench| {
        b_bench.iter(|| dot_product_u8(black_box(&a), black_box(&b)))
    });

    group.bench_function("euclidean_sq_u8", |b_bench| {
        b_bench.iter(|| euclidean_distance_sq_u8(black_box(&a), black_box(&b)))
    });

    group.bench_function("cosine_parts_u8", |b_bench| {
        b_bench.iter(|| cosine_similarity_parts_u8(black_box(&a), black_box(&b)))
    });

    group.finish();
}

criterion_group!(benches, bench_distances, bench_u8_distances);
criterion_main!(benches);
