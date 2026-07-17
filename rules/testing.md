# Testing Rules — Anti-Test-Mirroring

## Core Principle

**Every assertion must use a reference value computed independently of the code under test.**

A test that reconstructs the implementation's formula and asserts equality with the result
is a tautology — it proves the code does what the code does, not what it should do.

## Test-Mirroring Detection

```rust
// ❌ TEST-MIRRORING: assertion derived from implementation formula
let expected = (a - b).powi(2).sqrt();  // Same formula as compute()
assert!((result - expected).abs() < 1e-6);

// ✅ INDEPENDENT: reference value from external source or hand-calculated
// Euclidean distance of [1,0] and [0,1] is sqrt(2) ≈ 1.4142135
assert!((result - 1.4142135).abs() < 1e-4);
```

## Required Test Categories

For every module, tests must cover:

1. **Happy path** — normal input, expected output
2. **Empty input** — zero-length vectors, empty maps, nil transactions
3. **Boundary values** — `u64::MAX`, `f32::INFINITY`, dimension=1, dimension=0
4. **Error paths** — mismatched dimensions, corrupted data, invalid checksums
5. **Concurrency** — if the code uses locks/atomics, test with concurrent access

## Mutation Survival Check

Before marking a test suite as complete, ask:
> "If I changed `<` to `<=` or `+1` to `+0` in the implementation, would any test fail?"

If the answer is "no" for a logic branch, the test suite has a gap.

## Test Code Allowances

- `.unwrap()` and `.expect()` are **permitted** in test code (`#[cfg(test)]`)
- `panic!` in tests is acceptable for setup failures
- These allowances do NOT extend to production code called by tests
