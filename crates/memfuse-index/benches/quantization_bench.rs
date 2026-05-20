use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_index::quantize::ScalarQuantizer;
use rand::Rng;

fn bench_quantization(c: &mut Criterion) {
    let dim = 1536;
    let mut rng = rand::thread_rng();
    let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();

    // Pre-train a quantizer
    let v_ref = v.as_slice();
    let quantizer = ScalarQuantizer::train(&[v_ref], dim);
    let quantized = quantizer.quantize(&v);

    let mut group = c.benchmark_group("Quantization");

    group.bench_function("quantize", |b| b.iter(|| quantizer.quantize(black_box(&v))));

    group.bench_function("dequantize", |b| {
        b.iter(|| quantizer.dequantize(black_box(&quantized)))
    });

    group.finish();
}

criterion_group!(benches, bench_quantization);
criterion_main!(benches);
