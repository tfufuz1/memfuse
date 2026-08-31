use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_ollama::{build_rag_prompt, context_prefixer::truncate_prefix, xml_escape};

fn bench_xml_escape(c: &mut Criterion) {
    let mut group = c.benchmark_group("xml_escape");
    for size in [100, 1000, 10000, 100000].iter() {
        let input = "<tag attr=\"val\" alt='test'>& content</tag> ".repeat(size / 30);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| xml_escape(input));
        });
    }
    group.finish();
}

fn bench_build_rag_prompt(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_rag_prompt");
    let sys = "You are a helpful assistant.";
    let ctx = "Document context ".repeat(100);
    let query = "User query text";

    group.bench_function("default_rag_prompt", |b| {
        b.iter(|| build_rag_prompt(sys, &ctx, query));
    });
    group.finish();
}

fn bench_context_prefix_combination(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_prefix_combination");
    let prefix = "This chunk describes user preferences regarding database connections.";
    let chunk = "User prefers PostgreSQL with pool size 20.";

    group.bench_function("truncate_and_combine", |b| {
        b.iter(|| {
            let truncated = truncate_prefix(prefix, 50, 200);
            format!("{truncated}\n{chunk}")
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_xml_escape,
    bench_build_rag_prompt,
    bench_context_prefix_combination
);
criterion_main!(benches);
