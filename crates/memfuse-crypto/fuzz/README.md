# Fuzzing infrastructure for `memfuse-crypto`

This directory contains `cargo-fuzz` targets for `memfuse-crypto`.

## Targets

- `deletion_proof_tamper`: Generates a valid `DeletionProof`, applies arbitrary byte mutations, and verifies that bincode deserialization and `proof.verify()` never panic and safely return clean error/false results upon tampering.

## Running locally

```bash
cargo fuzz run deletion_proof_tamper
```

To run with a time limit (e.g. 60 seconds):

```bash
cargo fuzz run deletion_proof_tamper -- -max_total_time=60
```
