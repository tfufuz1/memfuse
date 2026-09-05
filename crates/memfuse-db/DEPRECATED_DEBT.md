# Deprecation Debt Inventory — `memfuse-db`

This document serves as a living inventory of local `#[allow(deprecated)]` usage within `crates/memfuse-db`.
It is reviewed whenever new deprecated-API call sites are introduced or modified, preventing silent re-introduction of crate-wide blanket deprecation suppression attributes (`#![allow(deprecated)]`), which previously occurred as an unintended side effect during refactoring (see git history `40330346` → `46a20b22` → subsequent fix).

---

## Call Sites Inventory

All current 40 local `#[allow(deprecated)]` call sites in `crates/memfuse-db/src/` are documented below:

### 1. `crates/memfuse-db/src/collection/search.rs`

- **Line 13**: `use crate::filter::MetadataFilter;`
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Import of deprecated legacy `MetadataFilter` type required for legacy search method signatures and internal conversions.
  - *Status*: Permanent internal wiring for backward compatibility.
- **Line 22**: `pub async fn search(&self, query_embedding: &[f32], k: usize) -> Result<Vec<SearchResult>>`
  - *Deprecated Item*: `Collection::search` / legacy search API
  - *Justification*: Implementation of deprecated legacy method `Collection::search` forwarding to `Collection::query()`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 40**: `pub async fn search_with_filter(&self, query: &[f32], k: usize, filter: Option<MetadataFilter>) -> Result<Vec<SearchResult>>`
  - *Deprecated Item*: `Collection::search_with_filter`
  - *Justification*: Implementation of deprecated legacy method `Collection::search_with_filter`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 57**: `pub async fn search_with_filter_expr(&self, query: &[f32], k: usize, filter: Option<FilterExpr>) -> Result<Vec<SearchResult>>`
  - *Deprecated Item*: `Collection::search_with_filter_expr`
  - *Justification*: Implementation of deprecated legacy method `Collection::search_with_filter_expr`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 114**: `pub async fn search_text(&self, query_text: &str, k: usize) -> Result<Vec<SearchResult>>`
  - *Deprecated Item*: `Collection::search_text`
  - *Justification*: Implementation of deprecated legacy method `Collection::search_text`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 233**: `pub async fn search_filtered(...)`
  - *Deprecated Item*: `Collection::search_filtered`
  - *Justification*: Implementation of deprecated legacy method `Collection::search_filtered`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 414**: `pub async fn hybrid_search(...)`
  - *Deprecated Item*: `Collection::hybrid_search`
  - *Justification*: Implementation of deprecated legacy method `Collection::hybrid_search`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 430**: `pub async fn hybrid_search_reranked(...)`
  - *Deprecated Item*: `Collection::hybrid_search_reranked`
  - *Justification*: Implementation of deprecated legacy method `Collection::hybrid_search_reranked`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 494**: `pub async fn hybrid_search_with_weights(...)`
  - *Deprecated Item*: `Collection::hybrid_search_with_weights`
  - *Justification*: Implementation of deprecated legacy method `Collection::hybrid_search_with_weights`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 510**: `pub async fn hybrid_search_with_strategy(...)`
  - *Deprecated Item*: `Collection::hybrid_search_with_strategy`
  - *Justification*: Implementation of deprecated legacy method `Collection::hybrid_search_with_strategy`.
  - *Status*: Deprecated legacy API surface (retained for backward compatibility).
- **Line 657**: `pub async fn hybrid_search_with_query(&self, query: &memfuse_core::HybridQuery) -> Result<Vec<crate::SearchResult>>`
  - *Deprecated Item*: `Collection::hybrid_search_with_query`
  - *Justification*: Internal implementation of low-level `hybrid_search_with_query` called directly by `HybridQueryBuilder::execute()`.
  - *Status*: Permanent internal implementation backing `Collection::query()`.

### 2. `crates/memfuse-db/src/collection/mod.rs`

- **Line 20**: `mod tests;`
  - *Deprecated Item*: Legacy search and filter methods tested in `tests.rs`
  - *Justification*: Suppresses deprecation warnings in the internal unit test submodule (`tests.rs`) which tests deprecated API methods for backward compatibility.
  - *Status*: Permanent test module suppression.

### 3. `crates/memfuse-db/src/collection/tests.rs`

- **Line 661**: `async fn test_collection_next_tx_sequence()`
  - *Deprecated Item*: Legacy `Collection::search`
  - *Justification*: Unit test exercises legacy API surface to ensure backward-compatibility guarantees remain intact across transaction sequences.
  - *Status*: Permanent test coverage.

### 4. `crates/memfuse-db/src/collection/query_builder.rs`

- **Line 8**: `use crate::filter::MetadataFilter;`
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Import of deprecated `MetadataFilter` in `HybridQueryBuilder` for `.metadata_filter()` builder compatibility method.
  - *Status*: Permanent internal wiring for builder backward compatibility.
- **Line 184**: `pub fn metadata_filter(mut self, filter: MetadataFilter) -> Self`
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Builder method accepting deprecated `MetadataFilter` and converting it to canonical `FilterExpr`.
  - *Status*: Permanent builder method for backward compatibility.
- **Line 323**: `let mut results = self.collection.hybrid_search_with_query(&hybrid_query).await?;`
  - *Deprecated Item*: `Collection::hybrid_search_with_query`
  - *Justification*: The `HybridQueryBuilder::execute()` builder API must call the underlying `hybrid_search_with_query` method directly since it IS the execution engine.
  - *Status*: Permanent internal wiring.
- **Line 450**: `let legacy = col.search(&[1.0, 0.0, 0.0, 0.0], 2).await.unwrap();` inside test `test_query_builder_matches_legacy_search`
  - *Deprecated Item*: `Collection::search`
  - *Justification*: Unit test verifying `HybridQueryBuilder` result parity against legacy `Collection::search`.
  - *Status*: Permanent test coverage.
- **Line 480**: `let legacy = col.search_with_filter_expr(...)` inside test `test_query_builder_matches_legacy_search_with_filter_expr`
  - *Deprecated Item*: `Collection::search_with_filter_expr`
  - *Justification*: Unit test verifying builder parity against legacy `Collection::search_with_filter_expr`.
  - *Status*: Permanent test coverage.
- **Line 519**: `let legacy = col.hybrid_search(...)` inside test `test_query_builder_matches_legacy_hybrid_search`
  - *Deprecated Item*: `Collection::hybrid_search`
  - *Justification*: Unit test verifying builder parity against legacy `Collection::hybrid_search`.
  - *Status*: Permanent test coverage.
- **Line 580**: `let legacy = col.hybrid_search_with_query(...)` inside test `test_query_builder_matches_legacy_hybrid_search_with_query`
  - *Deprecated Item*: `Collection::hybrid_search_with_query`
  - *Justification*: Unit test verifying builder parity against legacy `Collection::hybrid_search_with_query`.
  - *Status*: Permanent test coverage.

### 5. `crates/memfuse-db/src/lib.rs`

- **Line 108**: `pub use filter::MetadataFilter;`
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Re-export of deprecated `MetadataFilter` type at crate root for backward compatibility.
  - *Status*: Deprecated API re-export (retained for backward compatibility).
- **Line 801**: `pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>>`
  - *Deprecated Item*: `MemFuse::search` / default collection legacy search
  - *Justification*: `MemFuse` top-level facade method forwarding to default collection search query builder.
  - *Status*: Facade legacy API surface.
- **Line 818**: `pub async fn search_with_filter(...)`
  - *Deprecated Item*: `MemFuse::search_with_filter`
  - *Justification*: `MemFuse` top-level facade method for deprecated `search_with_filter`.
  - *Status*: Facade legacy API surface.
- **Line 835**: `pub async fn search_with_filter_expr(...)`
  - *Deprecated Item*: `MemFuse::search_with_filter_expr`
  - *Justification*: `MemFuse` top-level facade method forwarding to search builder with `FilterExpr`.
  - *Status*: Facade legacy API surface.
- **Line 880**: `pub async fn search_text(&self, text: &str, k: usize) -> Result<Vec<SearchResult>>`
  - *Deprecated Item*: `MemFuse::search_text`
  - *Justification*: `MemFuse` top-level facade method forwarding to text search builder.
  - *Status*: Facade legacy API surface.
- **Line 893**: `pub async fn search_filtered(...)`
  - *Deprecated Item*: `MemFuse::search_filtered`
  - *Justification*: `MemFuse` top-level facade method signature.
  - *Status*: Facade legacy API surface.
- **Line 901**: Invocation of `default_col().search_filtered(...)`
  - *Deprecated Item*: `Collection::search_filtered`
  - *Justification*: `MemFuse::search_filtered` implementation calling `Collection::search_filtered`.
  - *Status*: Facade legacy forwarding.
- **Line 909**: `pub async fn hybrid_search(...)`
  - *Deprecated Item*: `MemFuse::hybrid_search`
  - *Justification*: `MemFuse` top-level facade method forwarding to hybrid search builder.
  - *Status*: Facade legacy API surface.
- **Line 928**: `pub async fn hybrid_search_reranked(...)`
  - *Deprecated Item*: `MemFuse::hybrid_search_reranked`
  - *Justification*: `MemFuse` top-level facade method forwarding to reranked search builder.
  - *Status*: Facade legacy API surface.
- **Line 950**: `pub async fn hybrid_search_with_weights(...)`
  - *Deprecated Item*: `MemFuse::hybrid_search_with_weights`
  - *Justification*: `MemFuse` top-level facade method forwarding to weighted search builder.
  - *Status*: Facade legacy API surface.
- **Line 972**: `pub async fn hybrid_search_with_strategy(...)`
  - *Deprecated Item*: `MemFuse::hybrid_search_with_strategy`
  - *Justification*: `MemFuse` top-level facade method forwarding to strategy search builder.
  - *Status*: Facade legacy API surface.
- **Line 998**: `pub async fn hybrid_search_with_query(...)`
  - *Deprecated Item*: `MemFuse::hybrid_search_with_query`
  - *Justification*: `MemFuse` top-level facade method forwarding to query config search builder.
  - *Status*: Facade legacy API surface.
- **Line 1187**: `mod tests` block in `src/lib.rs`
  - *Deprecated Item*: Legacy `MemFuse` facade search methods
  - *Justification*: Unit tests module in `src/lib.rs` exercising legacy facade methods (`search`, `hybrid_search`, etc.) for backward compatibility test coverage.
  - *Status*: Permanent test module suppression.

### 6. `crates/memfuse-db/src/filter.rs`

- **Line 58**: `MetadataFilter::Condition` variant field definitions
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Field definitions inside deprecated `MetadataFilter` enum.
  - *Status*: Deprecated type definition.
- **Line 70**: `impl MetadataFilter` block
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Implementation block for deprecated `MetadataFilter` type (`eval` method).
  - *Status*: Deprecated type implementation.
- **Line 86**: `impl TryFrom<MetadataFilter> for FilterExpr` block
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: `TryFrom` conversion implementation mapping deprecated `MetadataFilter` to canonical `FilterExpr`.
  - *Status*: Permanent conversion logic for backward compatibility.
- **Line 159**: `fn test_filter_in_nin_exists_operators()`
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Unit test verifying legacy `MetadataFilter` operators.
  - *Status*: Permanent test coverage.
- **Line 214**: `fn test_filter_type_mismatch_safety()`
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Unit test verifying type-mismatch safety in legacy `MetadataFilter`.
  - *Status*: Permanent test coverage.
- **Line 251**: `fn test_try_from_empty_and_or_returns_invalid_input_error()`
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Unit test verifying `TryFrom<MetadataFilter>` error handling on empty AND/OR filters.
  - *Status*: Permanent test coverage.
- **Line 269**: `fn test_try_from_all_filter_ops()`
  - *Deprecated Item*: `MetadataFilter`
  - *Justification*: Unit test verifying `TryFrom<MetadataFilter>` conversion for all filter operators.
  - *Status*: Permanent test coverage.
