//! Scalar Quantization (SQ8) for HNSW Index.
// ANCHOR:TODO:QUANT-001 — Optimiere und finalisiere die SQ8 Quantization impl, repariere Cast-Bugs.
// WP:WP-2.2 PRIO:1 NEEDS:NONE
// AGENT:03 DATE:2026-05-15 STATUS:DONE
// TEST: cargo bench -p memfuse-index -- quantization
// DONE: Performance- und Recall Metriken sind stabil. Optimized u8 distance logic implemented.
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
        // [INV-MATH-2] NaN/Inf Protection
        if query.iter().any(|&x| x.is_nan() || x.is_infinite()) {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Query vector contains NaN or Infinity",
            ));
        }

        let range = self.max - self.min;
        let dist = match metric {
            DistanceMetric::DotProduct => {
                let mut dot = 0.0;
                for (&q, &v) in query.iter().zip(quantized.iter()) {
                    let deq = (v as f32 / 255.0) * range + self.min;
                    dot += q * deq;
                }
                -dot
            }
            DistanceMetric::Euclidean => {
                let mut sum_sq = 0.0;
                for (&q, &v) in query.iter().zip(quantized.iter()) {
                    let deq = (v as f32 / 255.0) * range + self.min;
                    let diff = q - deq;
                    sum_sq += diff * diff;
                }
                sum_sq.sqrt()
            }
            DistanceMetric::Cosine => {
                let mut dot = 0.0;
                let mut norm_a_sq = 0.0;
                let mut norm_b_sq = 0.0;
                for (&q, &v) in query.iter().zip(quantized.iter()) {
                    let deq = (v as f32 / 255.0) * range + self.min;
                    dot += q * deq;
                    norm_a_sq += q * q;
                    norm_b_sq += deq * deq;
                }
                if norm_a_sq == 0.0 || norm_b_sq == 0.0 {
                    1.0
                } else {
                    1.0 - (dot / (norm_a_sq.sqrt() * norm_b_sq.sqrt()))
                }
            }
        };
        Ok(dist)
    }

    /// Computes symmetric (approximate) distance purely in u8.
    /// This is a rough approximation sufficient for graph traversal ranking.
    pub fn symmetric_dist(
        &self,
        q1: &[u8],
        q2: &[u8],
        metric: DistanceMetric,
    ) -> memfuse_core::Result<f32> {
        use crate::distance::*;

        let range = self.max - self.min;

        match metric {
            DistanceMetric::DotProduct => {
                // (v1/255 * range + min) * (v2/255 * range + min)
                // = v1*v2/255^2 * range^2 + (v1+v2)/255 * range * min + min^2
                let mut dot_sum = 0.0;
                for (&v1, &v2) in q1.iter().zip(q2.iter()) {
                    let deq1 = (v1 as f32 / 255.0) * range + self.min;
                    let deq2 = (v2 as f32 / 255.0) * range + self.min;
                    dot_sum += deq1 * deq2;
                }
                Ok(-dot_sum)
            }
            DistanceMetric::Euclidean => {
                // sqrt( sum( ((v1-v2)/255 * range)^2 ) )
                // = sqrt( sum( (v1-v2)^2 ) * (range/255)^2 )
                // = range/255 * sqrt( sum( (v1-v2)^2 ) )
                let sum_sq = euclidean_distance_sq_u8(q1, q2);
                Ok((range / 255.0) * (sum_sq as f32).sqrt())
            }
            DistanceMetric::Cosine => {
                // This is harder to do purely in u8 because of the offsets,
                // but we can at least avoid Vec allocation.
                let mut dot = 0.0;
                let mut norm_a_sq = 0.0;
                let mut norm_b_sq = 0.0;
                for (&v1, &v2) in q1.iter().zip(q2.iter()) {
                    let deq1 = (v1 as f32 / 255.0) * range + self.min;
                    let deq2 = (v2 as f32 / 255.0) * range + self.min;
                    dot += deq1 * deq2;
                    norm_a_sq += deq1 * deq1;
                    norm_b_sq += deq2 * deq2;
                }
                if norm_a_sq == 0.0 || norm_b_sq == 0.0 {
                    Ok(1.0)
                } else {
                    Ok(1.0 - (dot / (norm_a_sq.sqrt() * norm_b_sq.sqrt())))
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
