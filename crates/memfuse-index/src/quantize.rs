//! Scalar Quantization (SQ8) for HNSW Index.
// ANCHOR:TODO:QUANT-001 — Optimiere und finalisiere die SQ8 Quantization impl, repariere Cast-Bugs.
// STATUS: DONE AGENT:03 DATE:2026-05-18
// WP:WP-2.2 PRIO:1 NEEDS:NONE
// AGENT:03 DATE:2026-05-18 STATUS:DONE
// TEST: cargo bench -p memfuse-index -- quantization
// DONE: Performance- und Recall Metriken sind stabil.
// SUCCESSOR: @JULES-05 — "SQ8 ist stabil. Nutze es nun als Vector-Signal im Hybrid Search."

use memfuse_core::DistanceMetric;
use std::simd::StdFloat;

/// An 8-bit Scalar Quantizer (SQ8) that maps `f32` vectors into `u8` bounds.
///
/// Quantization reduces the memory footprint of vector storage by 4x.
#[derive(Debug, Clone)]
pub struct ScalarQuantizer {
    pub min: f32,
    pub max: f32,
    pub scale: f32,
    pub inv_scale: f32,
    pub dimension: usize,
}

impl ScalarQuantizer {
    /// Creates a new ScalarQuantizer trained on a batch of vectors to find global min/max.
    pub fn train(batch: &[&[f32]], dimension: usize) -> Self {
        if batch.is_empty() {
            return Self {
                min: 0.0,
                max: 1.0,
                scale: 255.0,
                inv_scale: 1.0 / 255.0,
                dimension,
            };
        }

        let mut min = f32::MAX;
        let mut max = f32::MIN;

        for vec in batch {
            for &val in *vec {
                if val < min {
                    min = val;
                }
                if val > max {
                    max = val;
                }
            }
        }

        // Prevent div by zero if max == min
        if (max - min).abs() < f32::EPSILON {
            max = min + 1e-6;
        }

        let range = max - min;
        Self {
            min,
            max,
            scale: 255.0 / range,
            inv_scale: range / 255.0,
            dimension,
        }
    }

    /// Quantizes an `f32` vector to `u8`.
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8> {
        use std::simd::prelude::*;
        let mut i = 0;
        let n = vector.len();
        let mut res = Vec::with_capacity(n);

        let min_v = f32x8::splat(self.min);
        let max_v = f32x8::splat(self.max);
        let scale_v = f32x8::splat(self.scale);

        while i + 8 <= n {
            let v = f32x8::from_slice(&vector[i..i + 8]);
            let clamped = v.simd_clamp(min_v, max_v);
            let scaled = (clamped - min_v) * scale_v;
            // Portable SIMD round and cast
            let rounded = scaled.round();
            for j in 0..8 {
                res.push(rounded[j].clamp(0.0, 255.0) as u8);
            }
            i += 8;
        }

        while i < n {
            let v = vector[i].clamp(self.min, self.max);
            res.push(((v - self.min) * self.scale).round().clamp(0.0, 255.0) as u8);
            i += 1;
        }
        res
    }

    /// Dequantizes a `u8` vector back to `f32`.
    pub fn dequantize(&self, vector: &[u8]) -> Vec<f32> {
        use std::simd::prelude::*;
        let mut i = 0;
        let n = vector.len();
        let mut res = Vec::with_capacity(n);

        let inv_scale_v = f32x8::splat(self.inv_scale);
        let min_v = f32x8::splat(self.min);

        while i + 8 <= n {
            let chunk = u8x8::from_slice(&vector[i..i + 8]);
            let v = chunk.cast::<f32>();
            let dequant = v * inv_scale_v + min_v;
            res.extend_from_slice(dequant.as_array());
            i += 8;
        }

        while i < n {
            res.push((vector[i] as f32) * self.inv_scale + self.min);
            i += 1;
        }
        res
    }

    /// Computes the asymmetric distance between an exact query and a quantized vector.
    /// Optimized for zero allocations via inline dequantization.
    pub fn asymmetric_dist(
        &self,
        query: &[f32],
        quantized: &[u8],
        metric: DistanceMetric,
    ) -> memfuse_core::Result<f32> {
        use std::simd::prelude::*;
        if query.len() != quantized.len() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Vector dimensions must match",
            ));
        }

        let n = query.len();
        let inv_scale_v = f32x8::splat(self.inv_scale);
        let min_v = f32x8::splat(self.min);
        let mut i = 0;

        let mut dot_acc = f32x8::splat(0.0);
        let mut norm_a_acc = f32x8::splat(0.0);
        let mut norm_b_acc = f32x8::splat(0.0);
        let mut euc_acc = f32x8::splat(0.0);

        while i + 8 <= n {
            let x = f32x8::from_slice(&query[i..i + 8]);
            let mut y_f32 = [0.0f32; 8];
            for j in 0..8 {
                y_f32[j] = quantized[i + j] as f32;
            }
            let y_q = f32x8::from_array(y_f32);
            let y = y_q * inv_scale_v + min_v;

            match metric {
                DistanceMetric::Cosine => {
                    dot_acc += x * y;
                    norm_a_acc += x * x;
                    norm_b_acc += y * y;
                }
                DistanceMetric::Euclidean => {
                    let diff = x - y;
                    euc_acc += diff * diff;
                }
                DistanceMetric::DotProduct => {
                    dot_acc += x * y;
                }
            }
            i += 8;
        }

        let mut final_dot = dot_acc.reduce_sum();
        let mut final_norm_a = norm_a_acc.reduce_sum();
        let mut final_norm_b = norm_b_acc.reduce_sum();
        let mut final_euc = euc_acc.reduce_sum();

        while i < n {
            let x = query[i];
            let y = (quantized[i] as f32) * self.inv_scale + self.min;
            match metric {
                DistanceMetric::Cosine => {
                    final_dot += x * y;
                    final_norm_a += x * x;
                    final_norm_b += y * y;
                }
                DistanceMetric::Euclidean => {
                    let diff = x - y;
                    final_euc += diff * diff;
                }
                DistanceMetric::DotProduct => {
                    final_dot += x * y;
                }
            }
            i += 1;
        }

        match metric {
            DistanceMetric::Cosine => {
                if final_norm_a <= 0.0 || final_norm_b <= 0.0 {
                    Ok(1.0)
                } else {
                    Ok(1.0 - (final_dot / (final_norm_a.sqrt() * final_norm_b.sqrt())))
                }
            }
            DistanceMetric::Euclidean => Ok(final_euc.sqrt()),
            DistanceMetric::DotProduct => Ok(-final_dot),
        }
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

        match metric {
            DistanceMetric::Cosine => {
                let parts = crate::distance::cosine_similarity_parts_u8(q1, q2);
                let n = q1.len() as f32;
                let s2 = self.inv_scale * self.inv_scale;
                let sm = self.inv_scale * self.min;
                let m2 = self.min * self.min;

                // Dequantize the parts:
                // dot(x,y) = sum((qx_i * s + min) * (qy_i * s + min))
                //          = sum(qx_i*qy_i * s^2 + (qx_i+qy_i) * s * min + min^2)
                //          = s^2 * sum(qx_i*qy_i) + s * min * (sum(qx_i) + sum(qy_i)) + n * min^2
                let dot_f32 = s2 * (parts.dot as f32)
                    + sm * (parts.sum_a as f32 + parts.sum_b as f32)
                    + n * m2;

                let norm_a_sq_f32 =
                    s2 * (parts.norm_a_sq as f32) + sm * 2.0 * (parts.sum_a as f32) + n * m2;

                let norm_b_sq_f32 =
                    s2 * (parts.norm_b_sq as f32) + sm * 2.0 * (parts.sum_b as f32) + n * m2;

                if norm_a_sq_f32 <= 0.0 || norm_b_sq_f32 <= 0.0 {
                    Ok(1.0)
                } else {
                    Ok(1.0 - (dot_f32 / (norm_a_sq_f32.sqrt() * norm_b_sq_f32.sqrt())))
                }
            }
            DistanceMetric::Euclidean => {
                let dist_sq_u8 = crate::distance::euclidean_distance_sq_u8(q1, q2);
                // Euclidean distance dequantization:
                // dist(x,y)^2 = sum(((qx_i * s + min) - (qy_i * s + min))^2)
                //             = sum((s * (qx_i - qy_i))^2)
                //             = s^2 * sum((qx_i - qy_i)^2)
                let dist_sq_f32 = (self.inv_scale * self.inv_scale) * (dist_sq_u8 as f32);
                Ok(dist_sq_f32.sqrt())
            }
            DistanceMetric::DotProduct => {
                let parts = crate::distance::cosine_similarity_parts_u8(q1, q2);
                let n = q1.len() as f32;
                // Reuse cosine_similarity_parts_u8 for DotProduct to get sums efficiently
                let dot_f32 = (self.inv_scale * self.inv_scale) * (parts.dot as f32)
                    + (self.inv_scale * self.min) * (parts.sum_a as f32 + parts.sum_b as f32)
                    + n * (self.min * self.min);
                Ok(-dot_f32)
            }
        }
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

        let range = q.max - q.min;
        let mut max_err = 0.0_f32;

        for (a, b) in v1.iter().zip(dequant.iter()) {
            let err = (a - b).abs();
            if err > max_err {
                max_err = err;
            }
        }

        // Error should be strictly less than 1% of the range (actually around 1/255 = 0.39%)
        assert!(max_err < 0.01 * range);
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
        assert_eq!(q.min, 0.0);
        assert_eq!(q.max, 1.0);
        assert_eq!(q.dimension, 128);

        let v = vec![0.5; 128];
        let quantized = q.quantize(&v);
        assert_eq!(quantized.len(), 128);
        for &val in &quantized {
            assert!(val > 120 && val < 135); // Close to 127
        }
    }
}
