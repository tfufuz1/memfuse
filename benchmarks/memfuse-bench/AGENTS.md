# AGENTS.md — MemFuse Bench

## Purpose
`memfuse-bench` is the standalone, reproducible benchmark harness for measuring retrieval accuracy (Recall@K, MRR, error rate) across MemFuse features:
1. Context-Prefix Retrieval (Baseline vs. LLM/Rule-based Context-Prefix)
2. Cross-Encoder Reranking (RRF vs. Post-RRF Reranked)

## Invariants
- All benchmarks must execute on reproducible, deterministic synthetic corpora with annotated ground truth.
- Results must be output in both JSON (`results.json`) and Markdown format.
- Code in this crate must adhere to `#![forbid(unsafe_code)]` and zero unwrap policies in non-test paths where possible.
