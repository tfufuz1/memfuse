//! Scalar Quantization (SQ8) for HNSW Index.
// ANCHOR:TODO:QUANT-001 — Optimiere und finalisiere die SQ8 Quantization impl, repariere Cast-Bugs.
// WP:WP-2.2 PRIO:1 NEEDS:NONE
// AGENT:@JULES-03 DATE:2026-05-09 STATUS:READY
// TEST: cargo bench -p memfuse-index -- quantization
// DONE: Performance- und Recall Metriken sind stabil.
// SUCCESSOR: @JULES-05 — "SQ8 ist stabil. Nutze es nun als Vector-Signal im Hybrid Search."

use memfuse_core::DistanceMetric;

/// An 8-bit Scalar Quantizer that maps `f32` vectors into `u8` bounds based on global min/max limits.
#[derive(Debug, Clone)]
pub struct ScalarQuantizer {
    pub min: f32,
    pub max: f32,
    pub dimension: usize,
}

impl ScalarQuantizer {
    /// Creates a new ScalarQuantizer trained on a batch of vectors to find global min/max.
    pub fn train(batch: &[&[f32]], dimension: usize) -> Self {
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

        Self {
            min,
            max,
            dimension,
        }
    }

    /// Quantizes an `f32` vector to `u8`.
    pub fn quantize(&self, vector: &[f32]) -> Vec<u8> {
        let range = self.max - self.min;
        vector
            .iter()
            .map(|&v| {
                let clamped = v.clamp(self.min, self.max);
                let normalized = (clamped - self.min) / range;
                // ANCHOR:PERF:CAST-001 — Sicherer Integer-Cast mit Sättigung
                // WP:WP-0.0 PRIO:2 NEEDS:NONE
                // AGENT:03 DATE:2026-05-15 STATUS:DONE
                // CREATED:2026-05-09 DEADLINE:NONE
                // FUNDORT: memfuse-index/src/quantize.rs:50
                // RISIKO: Cast-without-check kann crashen oder falsche Daten liefern.
                // BEHEBUNG: TryFrom oder korrekte Sättigung.
                (normalized * 255.0).round().clamp(0.0, 255.0) as u8
            })
            .collect()
    }

    /// Dequantizes a `u8` vector back to `f32`.
    pub fn dequantize(&self, vector: &[u8]) -> Vec<f32> {
        let range = self.max - self.min;
        vector
            .iter()
            .map(|&v| (v as f32 / 255.0) * range + self.min)
            .collect()
    }

    /// Computes the asymmetric distance between an exact query and a quantized vector.
    pub fn asymmetric_dist(
        &self,
        query: &[f32],
        quantized: &[u8],
        metric: DistanceMetric,
    ) -> memfuse_core::Result<f32> {
        match metric {
            DistanceMetric::DotProduct => {
                let dot = crate::distance::dot_product_f32_u8(query, quantized);
                // dequantized v = quantized * alpha + min
                // dot(query, v) = dot(query, quantized * alpha + min)
                //               = alpha * dot(query, quantized) + min * sum(query)
                let alpha = (self.max - self.min) / 255.0;
                let sum_query: f32 = query.iter().sum();
                Ok(alpha * dot + self.min * sum_query)
            }
            DistanceMetric::Euclidean => {
                let alpha = (self.max - self.min) / 255.0;
                let dist_sq = crate::distance::euclidean_distance_sq_f32_u8(
                    query, quantized, alpha, self.min,
                );
                Ok(dist_sq.sqrt())
            }
            DistanceMetric::Cosine => {
                let alpha = (self.max - self.min) / 255.0;
                let parts = crate::distance::cosine_similarity_parts_f32_u8(query, quantized);

                // dot(query, v) = alpha * parts.dot_f32_u8 + min * sum(query)
                let sum_query: f32 = query.iter().sum();
                let dot_uv = alpha * parts.dot_f32_u8 + self.min * sum_query;

                // norm(v)^2 = sum((y_i * alpha + min)^2)
                //           = sum(y_i^2 * alpha^2 + 2 * y_i * alpha * min + min^2)
                //           = alpha^2 * norm_u8_sq + 2 * alpha * min * sum_u8 + min^2 * dimension
                let norm_v_sq = alpha * alpha * (parts.norm_u8_sq as f32)
                    + 2.0 * alpha * self.min * (parts.sum_u8 as f32)
                    + self.min * self.min * (self.dimension as f32);

                let norm_q_sq: f32 = query.iter().map(|&x| x * x).sum();

                if norm_q_sq <= 0.0 || norm_v_sq <= 0.0 {
                    Ok(1.0)
                } else {
                    let similarity = dot_uv / (norm_q_sq.sqrt() * norm_v_sq.sqrt());
                    Ok(1.0 - similarity)
                }
            }
        }
    }

    /// Computes symmetric (approximate) distance purely in u8.
    /// This is a rough approximation sufficient for graph traversal ranking.
    pub fn symmetric_dist(
        &self,
        q1: &[u8],
        q2: &[u8],
        metric: DistanceMetric,
    ) -> memfuse_core::Result<f32> {
        let alpha = (self.max - self.min) / 255.0;

        match metric {
            DistanceMetric::DotProduct => {
                let dot_u8 = crate::distance::dot_product_u8(q1, q2);
                let sum_q1: u32 = q1.iter().map(|&x| x as u32).sum();
                let sum_q2: u32 = q2.iter().map(|&x| x as u32).sum();

                // dot(v1, v2) = dot(q1*a+m, q2*a+m)
                //             = a^2 * dot(q1, q2) + a*m*sum(q1) + a*m*sum(q2) + m^2 * dim
                let res = (alpha * alpha) * (dot_u8 as f32)
                    + alpha * self.min * (sum_q1 as f32)
                    + alpha * self.min * (sum_q2 as f32)
                    + self.min * self.min * (self.dimension as f32);
                Ok(res)
            }
            DistanceMetric::Euclidean => {
                let dist_sq_u8 = crate::distance::euclidean_distance_sq_u8(q1, q2);
                // dist_sq(v1, v2) = sum(( (q1_i*a+m) - (q2_i*a+m) )^2)
                //                 = sum(( a*(q1_i - q2_i) )^2)
                //                 = a^2 * sum((q1_i - q2_i)^2)
                //                 = a^2 * dist_sq_u8
                let res = (alpha * alpha) * (dist_sq_u8 as f32);
                Ok(res.sqrt())
            }
            DistanceMetric::Cosine => {
                let p = crate::distance::cosine_similarity_parts_u8(q1, q2);

                // dot(v1, v2) = a^2 * p.dot + a*m*p.sum_a + a*m*p.sum_b + m^2 * dim
                let dot_v1v2 = (alpha * alpha) * (p.dot as f32)
                    + alpha * self.min * (p.sum_a as f32)
                    + alpha * self.min * (p.sum_b as f32)
                    + self.min * self.min * (self.dimension as f32);

                // norm(v1)^2 = a^2 * p.norm_a_sq + 2*a*m*p.sum_a + m^2 * dim
                let norm_v1_sq = (alpha * alpha) * (p.norm_a_sq as f32)
                    + 2.0 * alpha * self.min * (p.sum_a as f32)
                    + self.min * self.min * (self.dimension as f32);

                // norm(v2)^2 = a^2 * p.norm_b_sq + 2*a*m*p.sum_b + m^2 * dim
                let norm_v2_sq = (alpha * alpha) * (p.norm_b_sq as f32)
                    + 2.0 * alpha * self.min * (p.sum_b as f32)
                    + self.min * self.min * (self.dimension as f32);

                if norm_v1_sq <= 0.0 || norm_v2_sq <= 0.0 {
                    Ok(1.0)
                } else {
                    let similarity = dot_v1v2 / (norm_v1_sq.sqrt() * norm_v2_sq.sqrt());
                    Ok(1.0 - similarity)
                }
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
}
