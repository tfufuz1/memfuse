# Fuzzing infrastructure for `memfuse-db`

This directory contains `cargo-fuzz` targets for `memfuse-db`.

## Targets

- `rrf_fusion`: Fuzzes `weighted_reciprocal_rank_fusion` with arbitrary signal weights (including NaN/Infinity), result sets, and `max_results` values.

## Running locally

```bash
cargo fuzz run rrf_fusion
```

To run with a time limit (e.g. 60 seconds):

```bash
cargo fuzz run rrf_fusion -- -max_total_time=60
```
