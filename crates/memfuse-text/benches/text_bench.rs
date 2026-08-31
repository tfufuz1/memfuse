// FILE-CONTEXT: Performance Benchmark Suite for memfuse-text.
// ZWECK: Quantifiziert Durchsatz (Words/sec), Latencies p50/p95/p99 für Tokenisierungen, Morphologie & BM25-Suche.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_text::bm25::score_term;
use memfuse_text::morphology::{GermanCompoundSplitter, MorphologicalTokenizer};
use memfuse_text::tokenizer::{DefaultTokenizer, GermanMorphTokenizer, Tokenizer};

fn bench_tokenization(c: &mut Criterion) {
    let mut group = c.benchmark_group("tokenization");

    let text = "Das Bundesverfassungsgericht prüft den Urlaubsantragsprozess und die Qualitätsprüfung \
                in der Lebensversicherungsgesellschaft zur Einhaltung der Datenschutzrichtlinie.";
    let bytes_len = text.len() as u64;

    group.throughput(Throughput::Bytes(bytes_len));

    group.bench_function("default_tokenizer", |b| {
        let tok = DefaultTokenizer;
        b.iter(|| tok.tokenize(text));
    });

    group.bench_function("german_morph_tokenizer", |b| {
        let tok = GermanMorphTokenizer::new();
        b.iter(|| tok.tokenize(text));
    });

    group.finish();
}

fn bench_compound_splitting(c: &mut Criterion) {
    let mut group = c.benchmark_group("compound_splitting");
    let splitter = GermanCompoundSplitter::new();

    let words = [
        ("short_2_part", "arbeitsvertrag"),
        ("medium_3_part", "urlaubsantragsprozess"),
        ("long_3_part", "bundesverfassungsgericht"),
        ("extreme_4_part", "kraftfahrzeughaftpflichtversicherung"),
    ];

    for (label, word) in words {
        group.bench_with_input(BenchmarkId::new("decompose", label), &word, |b, w| {
            b.iter(|| splitter.decompose(w));
        });
    }

    group.finish();
}

fn bench_bm25_scoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("bm25_scoring");

    let corpus_sizes: &[u32] = &[1_000, 10_000, 100_000];

    for &n in corpus_sizes {
        group.bench_with_input(BenchmarkId::new("score_term_single", n), &n, |b, &n| {
            let df = (n / 20).max(1);
            let avg_doc_len = 120.0f32;
            b.iter(|| score_term(3, 110, avg_doc_len, df, n));
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_tokenization,
    bench_compound_splitting,
    bench_bm25_scoring
);
criterion_main!(benches);
