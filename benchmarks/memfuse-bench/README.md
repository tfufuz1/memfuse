# MemFuse Benchmark Harness (`memfuse-bench`)

This crate provides a reproducible evaluation harness for retrieval quality, contextual retrieval, cross-encoder reranking, and standardized long-term conversational memory benchmarks.

---

## 1. Running the Standard Synthetic Benchmark

The built-in synthetic benchmark evaluates contextual retrieval prefixing and cross-encoder reranking over a synthetic multi-domain corpus.

```bash
cargo run -p memfuse-bench -- synthetic
```

Or simply run without arguments (defaults to synthetic):

```bash
cargo run -p memfuse-bench
```

Output reports are written to `benchmarks/results/results.json` and `benchmarks/results/summary.md`.

---

## 2. Running External Benchmarks

### A. LongMemEval (ICLR 2025 / arXiv:2410.10813)

[LongMemEval](https://github.com/xiaowu0162/longmemeval) benchmarks chat assistants on long-term interactive memory across multi-session histories and evaluates question categories including single-session user/assistant/preference recall, knowledge updates, temporal reasoning, multi-session reasoning, and abstention.

#### How to Obtain the Dataset:
1. Download the official `longmemeval_s.jsonl` or `longmemeval_m.json` file from the official repository:
   - URL: `https://github.com/xiaowu0162/longmemeval`
2. Save or copy the dataset file into `benchmarks/memfuse-bench/data/longmemeval_s.jsonl` (or any custom path).

#### Execution:
```bash
cargo run -p memfuse-bench -- long-mem-eval --dataset path/to/longmemeval_s.jsonl
```

---

### B. LoCoMo (Long Conversational Memory Benchmark)

[LoCoMo](https://github.com/snap-research/locomo) (SNAP Research) evaluates memory systems over long multi-session dialogues across single-hop, multi-hop, temporal reasoning, open-domain, and adversarial question categories.

#### How to Obtain the Dataset:
1. Download `locomo10.json` from the official repository:
   - URL: `https://github.com/snap-research/locomo`
2. Save or copy the dataset file into `benchmarks/memfuse-bench/data/locomo10.json` (or any custom path).

#### Execution:
```bash
cargo run -p memfuse-bench -- locomo --dataset path/to/locomo10.json
```

---

## 3. Running Unit and Integration Tests

To run all tests including fixture parsers and mock search metric evaluations:

```bash
cargo test -p memfuse-bench
```
