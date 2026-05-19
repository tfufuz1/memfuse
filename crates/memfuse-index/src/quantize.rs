//! Scalar Quantization (SQ8) for HNSW Index.
// WP:WP-2.2 PRIO:1 NEEDS:NONE
// AGENT:03 DATE:2026-05-21 STATUS:DONE
// TEST: cargo bench -p memfuse-index -- quantization
// DONE: Performance- und Recall Metriken sind stabil.
// SUCCESSOR: @JULES-05 — "SQ8 ist stabil. Nutze es nun als Vector-Signal im Hybrid Search."

use memfuse_core::DistanceMetric;

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
        vector
            .iter()
            .map(|&v| {
                let clamped = v.clamp(self.min, self.max);
                // ANCHOR:PERF:CAST-001 — Sicherer Integer-Cast mit Sättigung
                // WP:WP-0.0 PRIO:2 NEEDS:NONE
                // AGENT:03 DATE:2026-05-16 STATUS:DONE
                // CREATED:2026-05-09 DEADLINE:NONE
                // FUNDORT: memfuse-index/src/quantize.rs
                // BEHEBUNG: Saturated casting via clamp and round.
                ((clamped - self.min) * self.scale)
                    .round()
                    .clamp(0.0, 255.0) as u8
            })
            .collect()
    }

    /// Dequantizes a `u8` vector back to `f32`.
    pub fn dequantize(&self, vector: &[u8]) -> Vec<f32> {
        vector
            .iter()
            .map(|&v| (v as f32) * self.inv_scale + self.min)
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
                let parts = crate::distance::cosine_similarity_parts_f32_u8(query, quantized);

                // Dequantize the u8 parts back to f32 space for the query norm
                let norm_a: f32 = query.iter().map(|x| x * x).sum();

                // norm_b = sum((y_i * inv_scale + min)^2)
                //        = sum(y_i^2 * inv_scale^2 + 2 * y_i * inv_scale * min + min^2)
                //        = norm_u8_sq * inv_scale^2 + 2 * sum_u8 * inv_scale * min + n * min^2
                let n = quantized.len() as f32;
                let norm_b = (parts.norm_u8_sq as f32) * self.inv_scale * self.inv_scale
                    + 2.0 * (parts.sum_u8 as f32) * self.inv_scale * self.min
                    + n * self.min * self.min;

                // dot = sum(x_i * (y_i * inv_scale + min))
                //     = sum(x_i * y_i * inv_scale + x_i * min)
                //     = dot_f32_u8 * inv_scale + sum_x * min
                let sum_x: f32 = query.iter().sum();
                let dot = parts.dot_f32_u8 * self.inv_scale + sum_x * self.min;

                if norm_a <= 0.0 || norm_b <= 0.0 {
                    1.0
                } else {
                    let sim = dot / (norm_a.sqrt() * norm_b.sqrt());
                    (1.0 - sim).clamp(0.0, 2.0)
                }
            }
            DistanceMetric::Euclidean => {
                // Euclidean distance = sqrt(sum((x_i - y_i)^2))
                // sum((x_i - (y_i * inv_scale + min))^2)
                crate::distance::euclidean_distance_sq_f32_u8(
                    query,
                    quantized,
                    self.inv_scale,
                    self.min,
                )
                .sqrt()
            }
            DistanceMetric::DotProduct => {
                // DotProduct distance = -sum(x_i * y_i)
                let sum_x: f32 = query.iter().sum();
                let dot_parts = crate::distance::cosine_similarity_parts_f32_u8(query, quantized);
                let dot = dot_parts.dot_f32_u8 * self.inv_scale + sum_x * self.min;
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

        let acc = match metric {
            DistanceMetric::Cosine => {
                let parts = crate::distance::cosine_similarity_parts_u8(q1, q2);
                let n = q1.len() as f32;

                // Expand (y_i * inv_scale + min) to compute dot and norms
                // dot = sum((x_qi * inv_scale + min) * (y_qi * inv_scale + min))
                //     = sum(x_qi*y_qi*inv_scale^2 + (x_qi+y_qi)*inv_scale*min + min^2)
                let dot = (parts.dot as f32) * self.inv_scale * self.inv_scale
                    + (parts.sum_a + parts.sum_b) as f32 * self.inv_scale * self.min
                    + n * self.min * self.min;

                let norm_a = (parts.norm_a_sq as f32) * self.inv_scale * self.inv_scale
                    + 2.0 * (parts.sum_a as f32) * self.inv_scale * self.min
                    + n * self.min * self.min;

                let norm_b = (parts.norm_b_sq as f32) * self.inv_scale * self.inv_scale
                    + 2.0 * (parts.sum_b as f32) * self.inv_scale * self.min
                    + n * self.min * self.min;

                if norm_a <= 0.0 || norm_b <= 0.0 {
                    1.0
                } else {
                    let sim = dot / (norm_a.sqrt() * norm_b.sqrt());
                    (1.0 - sim).clamp(0.0, 2.0)
                }
            }
            DistanceMetric::Euclidean => {
                // Euclidean distance = sqrt(sum((x_i - y_i)^2))
                // sum(((x_qi * inv_scale + min) - (y_qi * inv_scale + min))^2)
                // = sum((x_qi * inv_scale - y_qi * inv_scale)^2)
                // = inv_scale^2 * sum((x_qi - y_qi)^2)
                let dist_sq_u8 = crate::distance::euclidean_distance_sq_u8(q1, q2);
                ((dist_sq_u8 as f32) * self.inv_scale * self.inv_scale).sqrt()
            }
            DistanceMetric::DotProduct => {
                let parts = crate::distance::cosine_similarity_parts_u8(q1, q2);
                let n = q1.len() as f32;
                let dot = (parts.dot as f32) * self.inv_scale * self.inv_scale
                    + (parts.sum_a + parts.sum_b) as f32 * self.inv_scale * self.min
                    + n * self.min * self.min;
                -dot
            }
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
