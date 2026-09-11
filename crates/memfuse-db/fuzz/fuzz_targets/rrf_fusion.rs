#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use memfuse_db::fusion::weighted_reciprocal_rank_fusion;
use memfuse_db::SearchResult;

#[derive(Arbitrary, Debug)]
pub struct FuzzItem {
    pub id: String,
    pub score: f32,
}

#[derive(Arbitrary, Debug)]
pub struct FuzzSignalSet {
    pub signal_name: String,
    pub weight: f32,
    pub items: Vec<FuzzItem>,
}

#[derive(Arbitrary, Debug)]
pub struct RrfFuzzInput {
    pub sets: Vec<FuzzSignalSet>,
    pub max_results: usize,
}

fuzz_target!(|input: RrfFuzzInput| {
    let result_sets: Vec<(String, Vec<SearchResult>, f32)> = input
        .sets
        .into_iter()
        .map(|s| {
            let results = s
                .items
                .into_iter()
                .map(|item| SearchResult {
                    id: item.id,
                    score: item.score,
                    metadata: None,
                    matched_signals: Vec::new(),
                    provenance: None,
                })
                .collect();
            (s.signal_name, results, s.weight)
        })
        .collect();

    let fused = weighted_reciprocal_rank_fusion(result_sets, input.max_results);

    // Invariant checks
    assert!(
        fused.len() <= input.max_results,
        "Fused results length cannot exceed max_results"
    );
    for r in &fused {
        // Scores should be defined (not panic)
        let _ = r.score;
    }
});
