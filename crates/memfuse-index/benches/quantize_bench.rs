use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_index::quantize::ScalarQuantizer;
use rand::Rng;

fn bench_quantization(c: &mut Criterion) {
    let dim = 1536;
    let mut rng = rand::thread_rng();
    let vector: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();

    let quantizer = ScalarQuantizer {
        min: -1.0,
        max: 1.0,
        dimension: dim,
    };

    let mut group = c.benchmark_group("Quantization");

    group.bench_function("quantize_1536", |b| {
        b.iter(|| quantizer.quantize(black_box(&vector)))
    });

    let quantized = quantizer.quantize(&vector);
    group.bench_function("dequantize_1536", |b| {
        b.iter(|| quantizer.dequantize(black_box(&quantized)))
    });

    group.finish();
}

criterion_group!(benches, bench_quantization);
criterion_main!(benches);
