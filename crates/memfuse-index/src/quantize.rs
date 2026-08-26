//! Scalar Quantization (SQ8) for HNSW Index.

use crate::distance::euclidean_distance_sq_f32_u8;
use memfuse_core::DistanceMetric;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// An 8-bit Scalar Quantizer (SQ8) with per-dimension scaling.
///
/// Quantization reduces the memory footprint of vector storage by 4x.
/// Per-dimension scaling improves recall by adapting to different value ranges.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScalarQuantizer {
    pub mins: Vec<f32>,
    pub maxes: Vec<f32>,
    pub scales: Vec<f32>,
    pub inv_scales: Vec<f32>,
    pub dimension: usize,
    #[serde(skip, default)]
    pub total_queries: AtomicU64,
    #[serde(skip, default)]
    pub out_of_range_queries: AtomicU64,
}

impl Clone for ScalarQuantizer {
    fn clone(&self) -> Self {
        Self {
            mins: self.mins.clone(),
            maxes: self.maxes.clone(),
            scales: self.scales.clone(),
            inv_scales: self.inv_scales.clone(),
            dimension: self.dimension,
            total_queries: AtomicU64::new(self.total_queries.load(Ordering::Relaxed)),
            out_of_range_queries: AtomicU64::new(self.out_of_range_queries.load(Ordering::Relaxed)),
        }
    }
}

impl ScalarQuantizer {
    /// Creates a new ScalarQuantizer trained on a batch of vectors to find per-dimension min/max.
    ///
    /// For long-lived or growing collections, callers should periodically recalibrate the
    /// quantizer (e.g., during index rebuilds) using a representative sample of active vectors.
    /// Without recalibration, new vectors that fall outside the initial range will be clamped,
    /// leading to degraded quantization accuracy.
    pub fn train(batch: &[&[f32]], dimension: usize) -> Self {
        let mut mins = vec![f32::MAX; dimension];
        let mut maxes = vec![f32::MIN; dimension];

        if batch.is_empty() {
            return Self {
                mins: vec![0.0; dimension],
                maxes: vec![1.0; dimension],
                scales: vec![255.0; dimension],
                inv_scales: vec![1.0 / 255.0; dimension],
                dimension,
                total_queries: AtomicU64::new(0),
                out_of_range_queries: AtomicU64::new(0),
            };
        }

        for vec in batch {
            for (i, &val) in vec.iter().take(dimension).enumerate() {
                if val < mins[i] {
                    mins[i] = val;
                }
                if val > maxes[i] {
                    maxes[i] = val;
                }
            }
        }

        let mut scales = Vec::with_capacity(dimension);
        let mut inv_scales = Vec::with_capacity(dimension);

        for i in 0..dimension {
            // Prevent div by zero if max == min
            if (maxes[i] - mins[i]).abs() < f32::EPSILON {
                maxes[i] = mins[i] + 1e-6;
            }
            let range = maxes[i] - mins[i];
            scales.push(255.0 / range);
            inv_scales.push(range / 255.0);
        }

        Self {
            mins,
            maxes,
            scales,
            inv_scales,
            dimension,
            total_queries: AtomicU64::new(0),
            out_of_range_queries: AtomicU64::new(0),
        }
    }

    /// Calculates quantization drift as the fraction of dimensions falling outside [mins[i], maxes[i]].
    pub fn check_drift(&self, vector: &[f32]) -> f32 {
        if self.dimension == 0 || vector.is_empty() {
            return 0.0;
        }
        let out_count = vector
            .iter()
            .take(self.dimension)
            .enumerate()
            .filter(|(i, &v)| v < self.mins[*i] || v > self.maxes[*i])
            .count();
        out_count as f32 / self.dimension as f32
    }

    /// Expands mins/maxes to accommodate out-of-bounds vectors, recomputing scales.
    pub fn expand_bounds_to_fit(&mut self, vector: &[f32]) -> bool {
        let mut changed = false;
        for (i, &val) in vector.iter().take(self.dimension).enumerate() {
            if val < self.mins[i] {
                self.mins[i] = val;
                changed = true;
            }
            if val > self.maxes[i] {
                self.maxes[i] = val;
                changed = true;
            }
        }
        if changed {
            for i in 0..self.dimension {
                if (self.maxes[i] - self.mins[i]).abs() < f32::EPSILON {
                    self.maxes[i] = self.mins[i] + 1e-6;
                }
                let range = self.maxes[i] - self.mins[i];
                self.scales[i] = 255.0 / range;
                self.inv_scales[i] = range / 255.0;
            }
        }
        changed
    }

    /// Quantizes an `f32` vector to `u8`.
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8> {
        let mut is_out = false;
        for (i, &v) in vector.iter().enumerate().take(self.dimension) {
            if v < self.mins[i] || v > self.maxes[i] {
                is_out = true;
                break;
            }
        }

        let total = self.total_queries.fetch_add(1, Ordering::Relaxed) + 1;
        let out_cnt = if is_out {
            self.out_of_range_queries.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.out_of_range_queries.load(Ordering::Relaxed)
        };

        if total >= 20 && (out_cnt as f64 / total as f64) > 0.05 {
            let ratio = (out_cnt as f64 / total as f64) * 100.0;
            tracing::warn!(
                out_of_range_ratio = %format!("{:.1}%", ratio),
                total_queries = total,
                "Over 5% of quantized queries are outside the trained range — ScalarQuantizer recalibration is recommended."
            );
        }

        vector
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let clamped = v.clamp(self.mins[i], self.maxes[i]);
                ((clamped - self.mins[i]) * self.scales[i])
                    .round()
                    .clamp(0.0, 255.0) as u8
            })
            .collect()
    }

    /// Dequantizes a `u8` vector back to `f32`.
    pub fn dequantize(&self, vector: &[u8]) -> Vec<f32> {
        vector
            .iter()
            .enumerate()
            .map(|(i, &v)| (v as f32) * self.inv_scales[i] + self.mins[i])
            .collect()
    }

    /// Computes the asymmetric distance between an exact query and a quantized vector.
    /// Optimized for zero allocations via inline dequantization.
    pub fn asymmetric_dist(
        &self,
        query: &[f32],
        quantized: &[u8],
        metric: DistanceMetric,
    ) -> memfuse_core::Result<f32> {
        if query.len() != quantized.len() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Vector dimensions must match",
            ));
        }

        let acc = match metric {
            DistanceMetric::Cosine => {
                // For per-dimension scaling, we must dequantize each component
                let mut dot = 0.0;
                let mut norm_q_sq = 0.0;
                let mut norm_d_sq = 0.0;

                for i in 0..self.dimension {
                    let qi = query[i];
                    let di = (quantized[i] as f32) * self.inv_scales[i] + self.mins[i];
                    dot += qi * di;
                    norm_q_sq += qi * qi;
                    norm_d_sq += di * di;
                }

                if norm_q_sq <= 0.0 || norm_d_sq <= 0.0 {
                    1.0
                } else {
                    let sim = dot / (norm_q_sq.sqrt() * norm_d_sq.sqrt());
                    (1.0 - sim).max(0.0)
                }
            }
            DistanceMetric::Euclidean => {
                let dist_sq =
                    euclidean_distance_sq_f32_u8(query, quantized, &self.inv_scales, &self.mins);
                dist_sq.sqrt()
            }
            DistanceMetric::DotProduct => {
                let mut dot = 0.0;
                for i in 0..self.dimension {
                    let qi = query[i];
                    let di = (quantized[i] as f32) * self.inv_scales[i] + self.mins[i];
                    dot += qi * di;
                }
                -dot
            }
            _ => unreachable!(),
        };
        Ok(acc)
    }

    /// Computes symmetric (approximate) distance purely in u8.
    /// Optimized for zero allocations via inline dequantization.
    pub fn symmetric_dist(
        &self,
        q1: &[u8],
        q2: &[u8],
        metric: DistanceMetric,
    ) -> memfuse_core::Result<f32> {
        if q1.len() != q2.len() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Vector dimensions must match",
            ));
        }

        let mut dot = 0.0_f32;
        let mut norm_a_sq = 0.0_f32;
        let mut norm_b_sq = 0.0_f32;
        let mut dist_sq = 0.0_f32;

        for i in 0..self.dimension {
            let v1 = (q1[i] as f32) * self.inv_scales[i] + self.mins[i];
            let v2 = (q2[i] as f32) * self.inv_scales[i] + self.mins[i];
            dot += v1 * v2;
            norm_a_sq += v1 * v1;
            norm_b_sq += v2 * v2;
            dist_sq += (v1 - v2).powi(2);
        }

        let acc = match metric {
            DistanceMetric::Cosine => {
                if norm_a_sq <= 0.0 || norm_b_sq <= 0.0 {
                    1.0
                } else {
                    let sim = dot / (norm_a_sq.sqrt() * norm_b_sq.sqrt());
                    (1.0 - sim).max(0.0)
                }
            }
            DistanceMetric::Euclidean => dist_sq.sqrt(),
            DistanceMetric::DotProduct => -dot,
            _ => unreachable!(),
        };
        Ok(acc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_drift() {
        let v1 = vec![0.0, 0.0, 0.0, 0.0];
        let v2 = vec![10.0, 10.0, 10.0, 10.0];
        let q = ScalarQuantizer::train(&[v1.as_slice(), v2.as_slice()], 4);

        // Vector within range [0, 10] -> 0 drift
        let in_range = vec![0.5, 5.0, 2.0, 9.9];
        assert_eq!(q.check_drift(&in_range), 0.0);

        // 2 out of 4 dimensions out of range -> 0.5 drift
        let out_range = vec![-1.0, 5.0, 15.0, 3.0];
        assert_eq!(q.check_drift(&out_range), 0.5);
    }

    #[test]
    fn test_quantize_dequantize_roundtrip() {
        let v1 = vec![0.1, -0.5, 0.8, 1.2];
        let v2 = vec![-1.0, 0.0, 0.5, 2.0];

        let q = ScalarQuantizer::train(&[v1.as_slice(), v2.as_slice()], 4);

        let quant = q.quantize(&v1);
        let dequant = q.dequantize(&quant);

        let mut max_err = 0.0_f32;

        for i in 0..4 {
            let range = q.maxes[i] - q.mins[i];
            let err = (v1[i] - dequant[i]).abs();
            if err > max_err {
                max_err = err;
            }
            // Error should be strictly less than 1% of the per-dim range
            assert!(err < 0.01 * range);
        }
    }

    #[test]
    fn test_quantized_search_no_panic() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut vectors = Vec::new();
        for _ in 0..100 {
            let v: Vec<f32> = (0..128).map(|_| rng.gen_range(-1.0..1.0)).collect();
            vectors.push(v);
        }

        let refs: Vec<&[f32]> = vectors.iter().map(|v| v.as_slice()).collect();
        let q = ScalarQuantizer::train(&refs, 128);

        let q_vecs: Vec<Vec<u8>> = vectors.iter().map(|v| q.quantize(v)).collect();

        // Random queries
        for _ in 0..100 {
            let qv: Vec<f32> = (0..128).map(|_| rng.gen_range(-1.0..1.0)).collect();
            let qq = q.quantize(&qv);

            let mut top = 0;
            let mut top_dist = f32::MAX;
            for (i, v) in q_vecs.iter().enumerate() {
                let d = q
                    .symmetric_dist(&qq, v, DistanceMetric::Cosine)
                    .expect("dist"); // unwrap allowed in tests
                if d < top_dist {
                    top_dist = d;
                    top = i;
                }
            }
            assert!(top < 100);
        }
    }

    #[test]
    fn test_train_empty_batch() {
        let q = ScalarQuantizer::train(&[], 128);
        assert_eq!(q.mins[0], 0.0);
        assert_eq!(q.maxes[0], 1.0);
        assert_eq!(q.dimension, 128);

        let v = vec![0.5; 128];
        let quantized = q.quantize(&v);
        assert_eq!(quantized.len(), 128);
        for &val in &quantized {
            assert!(val > 120 && val < 135); // Close to 127
        }
    }

    /// Proves per-dimension SQ8 yields strictly better (or equal) reconstruction accuracy
    /// than a global min/max quantizer when dimensions have heterogeneous value ranges.
    ///
    /// # Anti-Mirroring: Reference values computed independently
    /// The global-quantizer MSE is computed using a *separately implemented* quantization
    /// formula (inline, without using `ScalarQuantizer`), making this a true regression check.
    ///
    /// # Invariant (FIND-IND-002)
    /// Per-dim MSE <= Global MSE, with strict inequality for heterogeneous distributions.
    #[test]
    fn test_per_dim_better_recall_than_global() {
        // Dimension 0: range [0.0, 1.0]
        // Dimension 1: range [0.0, 1000.0]
        // The wide range on dim 1 would dominate a global quantizer, degrading dim 0 recall.
        let vectors: Vec<Vec<f32>> = vec![
            vec![0.0, 0.0],
            vec![1.0, 1000.0],
            vec![0.5, 500.0],
            vec![0.1, 100.0],
            vec![0.9, 900.0],
        ];
        let refs: Vec<&[f32]> = vectors.iter().map(|v| v.as_slice()).collect();
        let per_dim_q = ScalarQuantizer::train(&refs, 2);

        // Probe vector: dim 0 has fine-grained variation (0.05), dim 1 is coarse.
        let probe = vec![0.05_f32, 50.0_f32];
        let per_dim_quant = per_dim_q.quantize(&probe);
        let per_dim_dequant = per_dim_q.dequantize(&per_dim_quant);

        // Compute per-dim MSE
        let per_dim_mse: f32 = probe
            .iter()
            .zip(per_dim_dequant.iter())
            .map(|(orig, recon)| (orig - recon).powi(2))
            .sum::<f32>()
            / probe.len() as f32;

        // Independently compute global quantizer MSE:
        // Global min = 0.0, global max = 1000.0 (derived from the training data above, not from ScalarQuantizer)
        let global_min = 0.0_f32;
        let global_max = 1000.0_f32;
        let global_range = global_max - global_min;
        let global_scale = 255.0 / global_range;
        let global_inv_scale = global_range / 255.0;

        let global_quant: Vec<u8> = probe
            .iter()
            .map(|&v| {
                ((v.clamp(global_min, global_max) - global_min) * global_scale)
                    .round()
                    .clamp(0.0, 255.0) as u8
            })
            .collect();

        let global_dequant: Vec<f32> = global_quant
            .iter()
            .map(|&q| (q as f32) * global_inv_scale + global_min)
            .collect();

        let global_mse: f32 = probe
            .iter()
            .zip(global_dequant.iter())
            .map(|(orig, recon)| (orig - recon).powi(2))
            .sum::<f32>()
            / probe.len() as f32;

        // Per-dim MSE must be strictly better than global MSE for heterogeneous data.
        // For dim 0 (range [0,1]): per-dim uses full 256 steps over 1.0 range (step ≈ 0.004)
        //                           global uses 256 steps over 1000.0 range (step ≈ 3.9) — terrible
        // This makes global_mse >> per_dim_mse by approximately (3.9/0.004)^2 ≈ 1M×
        assert!(
            per_dim_mse < global_mse,
            "Per-dim MSE ({}) should be < global MSE ({}) for heterogeneous dimensions",
            per_dim_mse,
            global_mse
        );

        // Concrete bound: per-dim reconstruction error on dim 0 must be < 1% of its range (0..1)
        let dim0_err = (probe[0] - per_dim_dequant[0]).abs();
        assert!(
            dim0_err < 0.01,
            "Per-dim quantizer should reconstruct dim0 (range 0..1) within 1%, got error: {}",
            dim0_err
        );
    }

    proptest::proptest! {
        /// Proptest: for any batch of vectors with dimension 2 where the ranges differ by
        /// at least 10×, per-dim SQ8 produces lower reconstruction error than global SQ8.
        ///
        /// # Anti-Mirroring: global quantizer reference is independently computed inline.
        /// # FIND-IND-002
        #[test]
        fn prop_per_dim_mse_le_global_mse(
            // dim0 values in [0, 1], dim1 values in [100, 1000] — heterogeneous ranges guaranteed
            dim0_vals in proptest::collection::vec(0.0f32..1.0f32, 2..10),
            dim1_vals in proptest::collection::vec(100.0f32..1000.0f32, 2..10),
        ) {
            let n = dim0_vals.len().min(dim1_vals.len());
            let vectors: Vec<Vec<f32>> = (0..n)
                .map(|i| vec![dim0_vals[i], dim1_vals[i]])
                .collect();
            let refs: Vec<&[f32]> = vectors.iter().map(|v| v.as_slice()).collect();
            let per_dim_q = ScalarQuantizer::train(&refs, 2);

            // Global quantizer: independently compute using min/max of ALL values across ALL dims
            let all_vals: Vec<f32> = vectors.iter().flat_map(|v| v.iter().copied()).collect();
            let g_min = all_vals.iter().cloned().fold(f32::MAX, f32::min);
            let g_max = all_vals.iter().cloned().fold(f32::MIN, f32::max);
            let g_range = if (g_max - g_min).abs() < f32::EPSILON { 1e-6 } else { g_max - g_min };
            let g_scale = 255.0 / g_range;
            let g_inv = g_range / 255.0;

            // Invariant 1: Step sizes (inv_scales) for per-dimension must be <= global step size (g_inv)
            for i in 0..2 {
                proptest::prop_assert!(
                    per_dim_q.inv_scales[i] <= g_inv + 1e-5,
                    "Dimension {} inv_scale {} should be <= global step size {}",
                    i, per_dim_q.inv_scales[i], g_inv
                );
            }

            // Invariant 2: Average MSE over 500 uniformly distributed random probe points
            // must be <= global average MSE (within a small statistical tolerance).
            // We use a simple deterministic LCG to avoid grid-alignment artifacts.
            let mut total_per_dim_mse = 0.0f32;
            let mut total_global_mse = 0.0f32;

            let mut seed = 42u32;
            let mut next_random = || {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                (seed & 0x7FFFFFFF) as f32 / 0x7FFFFFFF as f32
            };

            let num_probes = 500;
            let d0_min = vectors.iter().map(|v| v[0]).fold(f32::MAX, f32::min);
            let d0_max = vectors.iter().map(|v| v[0]).fold(f32::MIN, f32::max);
            let d0_range = if (d0_max - d0_min).abs() < f32::EPSILON { 0.0 } else { d0_max - d0_min };

            let d1_min = vectors.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
            let d1_max = vectors.iter().map(|v| v[1]).fold(f32::MIN, f32::max);
            let d1_range = if (d1_max - d1_min).abs() < f32::EPSILON { 0.0 } else { d1_max - d1_min };

            for _ in 0..num_probes {
                let p0 = d0_min + next_random() * d0_range;
                let p1 = d1_min + next_random() * d1_range;
                let probe = vec![p0, p1];

                // Per-dim error
                let per_dim_quant = per_dim_q.quantize(&probe);
                let per_dim_dequant = per_dim_q.dequantize(&per_dim_quant);
                let per_dim_mse: f32 = probe.iter().zip(per_dim_dequant.iter())
                    .map(|(o, r)| (o - r).powi(2)).sum::<f32>() / 2.0;

                // Global error
                let global_mse: f32 = probe.iter().map(|&v| {
                    let q = ((v.clamp(g_min, g_max) - g_min) * g_scale).round().clamp(0.0, 255.0) as u8;
                    let r = (q as f32) * g_inv + g_min;
                    (v - r).powi(2)
                }).sum::<f32>() / 2.0;

                total_per_dim_mse += per_dim_mse;
                total_global_mse += global_mse;
            }

            let avg_per_dim_mse = total_per_dim_mse / num_probes as f32;
            let avg_global_mse = total_global_mse / num_probes as f32;

            // With 500 uniform points, the statistical variance is extremely small,
            // but we allow a tiny 1e-3 tolerance for edge cases.
            proptest::prop_assert!(
                avg_per_dim_mse <= avg_global_mse + 1e-3,
                "Average per_dim_mse={} should be <= global_mse={}",
                avg_per_dim_mse, avg_global_mse
            );
        }
    }
}
