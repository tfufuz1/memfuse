use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_index::quantize::ScalarQuantizer;
use rand::Rng;

fn bench_quantization(c: &mut Criterion) {
    let dim = 1536;
    let mut rng = rand::thread_rng();
    let v: Vec<f32> = (0..dim).map(|_| rng.gen()).collect();

    let mut train_batch = Vec::new();
    for _ in 0..100 {
        let vec: Vec<f32> = (0..dim).map(|_| rng.gen()).collect();
        train_batch.push(vec);
    }
    let train_refs: Vec<&[f32]> = train_batch.iter().map(|v| v.as_slice()).collect();

    let quantizer = ScalarQuantizer::train(&train_refs, dim);
    let quantized = quantizer.quantize(&v);

    let mut group = c.benchmark_group("Quantization");

    group.bench_function("quantize", |b| {
        b.iter(|| quantizer.quantize(black_box(&v)))
    });

    group.bench_function("dequantize", |b| {
        b.iter(|| quantizer.dequantize(black_box(&quantized)))
    });

    group.finish();
}

criterion_group!(benches, bench_quantization);
criterion_main!(benches);
