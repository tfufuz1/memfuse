//! Reciprocal Rank Fusion (RRF) for combining search results.
//!
//! RRF serves as the standard fusion technique in Project Chimera, abstracting
//! over heterogeneous scoring metrics (vector cosine similarity vs graph hops).

use ahash::AHashMap;
use chimera_core::{DocId, ScoredDocument};
use std::cmp::Ordering;

/// Configuration for RRF Fusion.
#[derive(Debug, Clone, Copy)]
pub struct RRFConfig {
    /// Smoothing constant (default: 60).
    pub k: f32,
}

impl Default for RRFConfig {
    fn default() -> Self {
        Self { k: 60.0 }
    }
}

/// RRF fusion engine.
pub struct RRFFusion {
    config: RRFConfig,
}

impl RRFFusion {
    /// Creates a new RRF fusion with default configuration (k=60).
    pub fn new() -> Self {
        Self {
            config: RRFConfig::default(),
        }
    }

    /// Creates a new RRF fusion with custom configuration.
    pub fn with_config(config: RRFConfig) -> Self {
        Self { config }
    }

    /// Fuses multiple ranked lists using standard RRF.
    ///
    /// # Arguments
    /// * `lists` - Multiple ranked lists of scored documents
    pub fn fuse(&self, lists: Vec<Vec<ScoredDocument>>) -> Vec<ScoredDocument> {
        // Default weight of 1.0 for all lists
        let weighted_lists: Vec<(Vec<ScoredDocument>, f32)> =
            lists.into_iter().map(|list| (list, 1.0)).collect();

        self.fuse_weighted(weighted_lists)
    }

    /// Fuses multiple ranked lists using Weighted RRF.
    ///
    /// This allows prioritizing certain sources (e.g., Graph > Vector) as
    /// suggested in the architectural optimization plan.
    ///
    /// Formula: Score = sum( weight * (1.0 / (k + rank)) )
    pub fn fuse_weighted(&self, lists: Vec<(Vec<ScoredDocument>, f32)>) -> Vec<ScoredDocument> {
        if lists.is_empty() {
            return Vec::new();
        }

        // Estimate capacity to reduce re-allocations
        let estimated_docs = lists.iter().map(|(l, _)| l.len()).sum();
        let mut scores: AHashMap<DocId, f32> = AHashMap::with_capacity(estimated_docs);
        let mut docs: AHashMap<DocId, ScoredDocument> = AHashMap::with_capacity(estimated_docs);

        for (list, weight) in lists {
            for (rank, doc) in list.into_iter().enumerate() {
                // RRF Formula: weight * (1.0 / (k + rank))
                // Rank is 1-based in the formula.
                // We use (rank + 1.0) because enumerate() starts at 0.
                let rrf_score = weight * (1.0 / (self.config.k + (rank as f32) + 1.0));

                let score_entry = scores.entry(doc.doc_id).or_insert(0.0);
                *score_entry += rrf_score;

                // Store document payload (first writer wins logic for document content)
                // [SPEC-039] Optimization: Only clone if not already present
                if !docs.contains_key(&doc.doc_id) {
                    docs.insert(doc.doc_id, doc);
                }
            }
        }

        // Create final sorted list
        let mut results: Vec<ScoredDocument> = Vec::with_capacity(scores.len());
        for (doc_id, score) in scores {
            if let Some(mut doc) = docs.remove(&doc_id) {
                doc.score = score;
                results.push(doc);
            }
        }

        // Sort by score (descending), then DocId (ascending) for determinism
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });

        results
    }
}

impl Default for RRFFusion {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a dummy document with a specific ID
    fn doc(id: u64) -> ScoredDocument {
        ScoredDocument::new(DocId::new(id), 1.0) // Score doesn't matter for RRF input, only rank
    }

    #[test]
    fn test_metaphor_expert_search() {
        // TDD Scenario: "Expert Search"
        // 1. "Sympathie" (Vector)
        // 2. "Qualifikation" (Metadata)
        // 3. "Berufserfahrung" (Graph)

        let fusion = RRFFusion::new();

        // Candidates
        let cand_consistent = DocId::new(1); // The "Safe Choice" - Top 10 everywhere
        let cand_specialist = DocId::new(2); // The "Specialist" - #1 in one list, absent in others
        let cand_mediocre = DocId::new(3); // The "Mediocre" - #20 everywhere

        // List 1: Sympathie (Vector)
        // Specialist is #1 (Rank 1), Consistent is #5 (Rank 5), Mediocre is #200
        let mut list_sympathy = vec![
            doc(cand_specialist.inner()), // Rank 1
            doc(101),
            doc(102),
            doc(103),                     // Fillers
            doc(cand_consistent.inner()), // Rank 5
        ];
        for i in 6..200 {
            list_sympathy.push(doc(1000 + i as u64));
        }
        list_sympathy.push(doc(cand_mediocre.inner())); // Rank 200

        // List 2: Qualifikation (Metadata)
        // Consistent is #2 (Rank 2), Specialist is NOT present, Mediocre is #200
        let mut list_qualification = vec![
            doc(201),                     // Rank 1
            doc(cand_consistent.inner()), // Rank 2
        ];
        for i in 3..200 {
            list_qualification.push(doc(2000 + i as u64));
        }
        list_qualification.push(doc(cand_mediocre.inner())); // Rank 200

        // List 3: Berufserfahrung (Graph)
        // Consistent is #8 (Rank 8), Specialist is NOT present, Mediocre is #200
        let mut list_experience = vec![
            doc(301),
            doc(302),
            doc(303),
            doc(304),
            doc(305),
            doc(306),
            doc(307),
            doc(cand_consistent.inner()), // Rank 8
        ];
        for i in 9..200 {
            list_experience.push(doc(3000 + i as u64));
        }
        list_experience.push(doc(cand_mediocre.inner())); // Rank 200

        // Execute Fusion
        let results = fusion.fuse(vec![list_sympathy, list_qualification, list_experience]);

        // Verification Logic
        // ... previous logic ...

        // Assertion: Consistent Candidate MUST win over Specialist
        assert_eq!(
            results[0].doc_id, cand_consistent,
            "The 'Consistent' candidate (Top 10 everywhere) should win"
        );
        assert_eq!(
            results[1].doc_id, cand_specialist,
            "Specialist should be second"
        );

        // Find Mediocre in results
        let mediocre_result = results
            .iter()
            .find(|r| r.doc_id == cand_mediocre)
            .expect("Mediocre should be found");
        assert!(
            results[1].score > mediocre_result.score,
            "Specialist must rank higher than Mediocre"
        );

        println!(
            "Winner: {:?} with score {}",
            results[0].doc_id, results[0].score
        );
        println!(
            "Runner-up: {:?} with score {}",
            results[1].doc_id, results[1].score
        );
        println!(
            "Mediocre: {:?} with score {}",
            mediocre_result.doc_id, mediocre_result.score
        );
    }

    #[test]
    fn test_rrf_mathematical_correctness() {
        let fusion = RRFFusion::new();
        let k = 60.0;

        // Document A is Rank 1 (index 0)
        let list1 = vec![doc(1)];

        let results = fusion.fuse(vec![list1]);

        let expected_score = 1.0 / (k + 1.0);
        assert!(
            (results[0].score - expected_score).abs() < f32::EPSILON,
            "Score calculation must match formula 1/(k+rank)"
        );
    }

    #[test]
    fn test_ranking_boost_intersection() {
        // Documents appearing in multiple lists should be boosted
        let fusion = RRFFusion::new();

        let list1 = vec![doc(1), doc(2)];
        let list2 = vec![doc(2), doc(3)]; // Doc 2 appears again

        let results = fusion.fuse(vec![list1, list2]);

        // Doc 2 should be first because it accumulates scores from both lists
        assert_eq!(results[0].doc_id, DocId::new(2));
    }
}
