//! Scalar Quantization (SQ8) for HNSW Index.
// WP:WP-2.2 PRIO:1 NEEDS:NONE

use crate::distance::euclidean_distance_sq_f32_u8;
use memfuse_core::DistanceMetric;

use serde::{Deserialize, Serialize};

/// An 8-bit Scalar Quantizer (SQ8) with per-dimension scaling.
///
/// Quantization reduces the memory footprint of vector storage by 4x.
/// Per-dimension scaling improves recall by adapting to different value ranges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarQuantizer {
    pub mins: Vec<f32>,
    pub maxes: Vec<f32>,
    pub scales: Vec<f32>,
    pub inv_scales: Vec<f32>,
    pub dimension: usize,
}

impl ScalarQuantizer {
    /// Creates a new ScalarQuantizer trained on a batch of vectors to find per-dimension min/max.
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
            };
        }

        for vec in batch {
            for i in 0..dimension.min(vec.len()) {
                let val = vec[i];
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
        }
    }

    /// Quantizes an `f32` vector to `u8`.
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8> {
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
        };
        Ok(acc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
