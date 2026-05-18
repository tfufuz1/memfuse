use memfuse_core::DistanceMetric;
use std::simd::prelude::*;
use std::simd::StdFloat;
#[derive(Debug, Clone)]
pub struct ScalarQuantizer {
    pub min: f32,
    pub max: f32,
    pub scale: f32,
    pub inv_scale: f32,
    pub dimension: usize,
}
impl ScalarQuantizer {
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
        let (mut min, mut max) = (f32::MAX, f32::MIN);
        for v in batch {
            for &x in *v {
                if x < min {
                    min = x;
                }
                if x > max {
                    max = x;
                }
            }
        }
        if (max - min).abs() < 1e-6 {
            max = min + 1e-6;
        }
        let r = max - min;
        Self {
            min,
            max,
            scale: 255.0 / r,
            inv_scale: r / 255.0,
            dimension,
        }
    }
    pub fn quantize(&self, v: &[f32]) -> Vec<u8> {
        let mut res = Vec::with_capacity(v.len());
        let mut i = 0;
        let (minv, maxv, sv, zero, top) = (
            f32x8::splat(self.min),
            f32x8::splat(self.max),
            f32x8::splat(self.scale),
            f32x8::splat(0.0),
            f32x8::splat(255.0),
        );
        while i + 8 <= v.len() {
            let chunk = f32x8::from_slice(&v[i..i + 8]);
            let q = ((chunk.simd_clamp(minv, maxv) - minv) * sv)
                .round()
                .simd_clamp(zero, top)
                .cast::<u32>();
            for j in 0..8 {
                res.push(q[j] as u8);
            }
            i += 8;
        }
        while i < v.len() {
            res.push(
                ((v[i].clamp(self.min, self.max) - self.min) * self.scale)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            );
            i += 1;
        }
        res
    }
    pub fn dequantize(&self, v: &[u8]) -> Vec<f32> {
        let mut res = Vec::with_capacity(v.len());
        let mut i = 0;
        let (isv, minv) = (f32x8::splat(self.inv_scale), f32x8::splat(self.min));
        while i + 8 <= v.len() {
            let mut c = [0f32; 8];
            for j in 0..8 {
                c[j] = v[i + j] as f32;
            }
            res.extend_from_slice((f32x8::from_array(c) * isv + minv).as_array());
            i += 8;
        }
        while i < v.len() {
            res.push((v[i] as f32) * self.inv_scale + self.min);
            i += 1;
        }
        res
    }
    pub fn asymmetric_dist(
        &self,
        query: &[f32],
        quantized: &[u8],
        metric: DistanceMetric,
    ) -> memfuse_core::Result<f32> {
        let acc = match metric {
            DistanceMetric::Cosine => {
                let p = crate::distance::cosine_similarity_parts_f32_u8(query, quantized);
                let na = query.iter().map(|&x| x * x).sum::<f32>();
                let dot = p.dot_f32_u8 * self.inv_scale + (query.iter().sum::<f32>() * self.min);
                let nb = (p.norm_u8_sq as f32) * self.inv_scale * self.inv_scale
                    + 2.0 * self.inv_scale * self.min * (p.sum_u8 as f32)
                    + (query.len() as f32) * self.min * self.min;
                if na == 0.0 || nb == 0.0 {
                    1.0
                } else {
                    1.0 - (dot / (na.sqrt() * nb.sqrt()))
                }
            }
            DistanceMetric::Euclidean => crate::distance::euclidean_distance_sq_f32_u8(
                query,
                quantized,
                self.inv_scale,
                self.min,
            )
            .sqrt(),
            DistanceMetric::DotProduct => {
                -(crate::distance::dot_product_f32_u8(query, quantized) * self.inv_scale
                    + (query.iter().sum::<f32>() * self.min))
            }
        };
        Ok(acc)
    }
    pub fn symmetric_dist(
        &self,
        q1: &[u8],
        q2: &[u8],
        metric: DistanceMetric,
    ) -> memfuse_core::Result<f32> {
        let acc = match metric {
            DistanceMetric::Cosine => {
                let p = crate::distance::cosine_similarity_parts_u8(q1, q2);
                let dot = (p.dot as f32) * self.inv_scale * self.inv_scale
                    + self.inv_scale * self.min * (p.sum_a as f32 + p.sum_b as f32)
                    + (q1.len() as f32) * self.min * self.min;
                let na = (p.norm_a_sq as f32) * self.inv_scale * self.inv_scale
                    + 2.0 * self.inv_scale * self.min * (p.sum_a as f32)
                    + (q1.len() as f32) * self.min * self.min;
                let nb = (p.norm_b_sq as f32) * self.inv_scale * self.inv_scale
                    + 2.0 * self.inv_scale * self.min * (p.sum_b as f32)
                    + (q1.len() as f32) * self.min * self.min;
                if na <= 0.0 || nb <= 0.0 {
                    1.0
                } else {
                    1.0 - (dot / (na.sqrt() * nb.sqrt()))
                }
            }
            DistanceMetric::Euclidean => ((crate::distance::euclidean_distance_sq_u8(q1, q2)
                as f32)
                * self.inv_scale
                * self.inv_scale)
                .sqrt(),
            DistanceMetric::DotProduct => {
                -(q1.iter()
                    .zip(q2.iter())
                    .map(|(&x, &y)| {
                        (x as f32 * self.inv_scale + self.min)
                            * (y as f32 * self.inv_scale + self.min)
                    })
                    .sum::<f32>())
            }
        };
        Ok(acc)
    }
}
