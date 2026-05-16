//! Scalar Quantization (SQ8) for HNSW Index.
// ANCHOR:TODO:QUANT-001 — SQ8 Quantization ist optimiert (SIMD) und finalisiert.
// WP:WP-2.2 PRIO:1 NEEDS:NONE
// AGENT:@JULES-03 DATE:2026-05-16 STATUS:DONE
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
    pub inv_255_range: f32,
    pub dimension: usize,
}

impl ScalarQuantizer {
    /// Creates a new ScalarQuantizer trained on a batch of vectors to find global min/max.
    pub fn train(batch: &[&[f32]], dimension: usize) -> Self {
        if batch.is_empty() {
            return Self {
                min: 0.0,
                max: 1.0,
                inv_255_range: 1.0 / 255.0,
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
            inv_255_range: range / 255.0,
            dimension,
        }
    }

    /// Quantizes an `f32` vector to `u8`.
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8> {
        let range = self.max - self.min;
        let inv_range = if range > 0.0 { 1.0 / range } else { 0.0 };
        vector
            .iter()
            .map(|&v| {
                let clamped = v.clamp(self.min, self.max);
                let normalized = (clamped - self.min) * inv_range;
                // ANCHOR:PERF:CAST-001 — Sicherer Integer-Cast mit Sättigung
                // WP:WP-0.0 PRIO:2 NEEDS:NONE
                // AGENT:03 DATE:2026-05-15 STATUS:DONE
                // CREATED:2026-05-09 DEADLINE:NONE
                // FUNDORT: memfuse-index/src/quantize.rs:50
                // RISIKO: Cast-without-check kann crashen oder falsche Daten liefern.
                // BEHEBUNG: TryFrom oder korrekte Sättigung.
                let val = (normalized * 255.0).round();
                if val.is_nan() || val <= 0.0 {
                    0
                } else if val >= 255.0 {
                    255
                } else {
                    val as u8
                }
            })
            .collect()
    }

    /// Dequantizes a `u8` vector back to `f32`.
    pub fn dequantize(&self, vector: &[u8]) -> Vec<f32> {
        vector
            .iter()
            .map(|&v| (v as f32) * self.inv_255_range + self.min)
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

        match metric {
            DistanceMetric::Cosine => {
                let parts = crate::distance::cosine_similarity_parts_f32_u8(query, quantized);
                // De-quantize the components needed for cosine
                // dot(f32, dequant(u8)) = dot(f32, u8 * alpha + min) = alpha * dot(f32, u8) + min * sum(f32)
                let sum_f32: f32 = query.iter().sum();
                let dot = self.inv_255_range * parts.dot_f32_u8 + self.min * sum_f32;

                // norm_u8_sq = sum((u8 * alpha + min)^2) = sum(u8^2 * alpha^2 + 2 * u8 * alpha * min + min^2)
                //           = alpha^2 * sum(u8^2) + 2 * alpha * min * sum(u8) + n * min^2
                let n = quantized.len() as f32;
                let norm_b_sq = self.inv_255_range * self.inv_255_range * parts.norm_u8_sq as f32
                    + 2.0 * self.inv_255_range * self.min * parts.sum_u8 as f32
                    + n * self.min * self.min;

                let norm_a_sq: f32 = query.iter().map(|x| x * x).sum();

                if norm_a_sq <= 0.0 || norm_b_sq <= 0.0 {
                    Ok(1.0)
                } else {
                    Ok(1.0 - (dot / (norm_a_sq.sqrt() * norm_b_sq.sqrt())))
                }
            }
            DistanceMetric::Euclidean => {
                let dist_sq = crate::distance::euclidean_distance_sq_f32_u8(
                    query,
                    quantized,
                    self.inv_255_range,
                    self.min,
                );
                Ok(dist_sq.sqrt())
            }
            DistanceMetric::DotProduct => {
                let dot_f32_u8 = crate::distance::dot_product_f32_u8(query, quantized);
                let sum_f32: f32 = query.iter().sum();
                let dot = self.inv_255_range * dot_f32_u8 + self.min * sum_f32;
                Ok(-dot)
            }
        }
    }

    /// Computes symmetric (approximate) distance purely in u8.
    /// Optimized for zero allocations via specialized u8 metrics.
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
                let alpha = self.inv_255_range;
                let alpha_sq = alpha * alpha;
                let min = self.min;

                // dot = sum((u8_a * alpha + min) * (u8_b * alpha + min))
                //     = sum(u8_a * u8_b * alpha^2 + (u8_a + u8_b) * alpha * min + min^2)
                //     = alpha^2 * dot_u8 + alpha * min * (sum_a + sum_b) + n * min^2
                let dot = alpha_sq * parts.dot as f32
                    + alpha * min * (parts.sum_a + parts.sum_b) as f32
                    + n * min * min;

                // norm^2 = sum((u8 * alpha + min)^2) = alpha^2 * sum(u8^2) + 2 * alpha * min * sum(u8) + n * min^2
                let norm_a_sq = alpha_sq * parts.norm_a_sq as f32
                    + 2.0 * alpha * min * parts.sum_a as f32
                    + n * min * min;
                let norm_b_sq = alpha_sq * parts.norm_b_sq as f32
                    + 2.0 * alpha * min * parts.sum_b as f32
                    + n * min * min;

                if norm_a_sq <= 0.0 || norm_b_sq <= 0.0 {
                    Ok(1.0)
                } else {
                    Ok(1.0 - (dot / (norm_a_sq.sqrt() * norm_b_sq.sqrt())))
                }
            }
            DistanceMetric::Euclidean => {
                let dist_sq_u8 = crate::distance::euclidean_distance_sq_u8(q1, q2);
                // Euclidean distance is scale-invariant for u8 differences
                // sum((x_i * alpha + min - (y_i * alpha + min))^2) = sum((alpha * (x_i - y_i))^2) = alpha^2 * sum((x_i - y_i)^2)
                Ok((self.inv_255_range * self.inv_255_range * dist_sq_u8 as f32).sqrt())
            }
            DistanceMetric::DotProduct => {
                let n = q1.len() as f32;
                let alpha = self.inv_255_range;
                let min = self.min;

                // dot = sum((u8_a * alpha + min) * (u8_b * alpha + min))
                //     = alpha^2 * dot_u8 + alpha * min * (sum_a + sum_b) + n * min^2
                // We need dot_u8, sum_a and sum_b, all provided by cosine_similarity_parts_u8
                let parts = crate::distance::cosine_similarity_parts_u8(q1, q2);
                let dot = (alpha * alpha) * parts.dot as f32
                    + alpha * min * (parts.sum_a + parts.sum_b) as f32
                    + n * min * min;
                Ok(-dot)
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
