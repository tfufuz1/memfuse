use memfuse_core::DistanceMetric;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::simd::prelude::*;
#[inline]
pub fn compute_distance(a: &[f32], b: &[f32], m: DistanceMetric) -> memfuse_core::Result<f32> {
    if a.len() != b.len() {
        return Err(memfuse_core::MemFuseError::invalid_input("match"));
    }
    Ok(match m {
        DistanceMetric::Cosine => cosine_distance(a, b),
        DistanceMetric::Euclidean => euclidean_distance(a, b),
        DistanceMetric::DotProduct => dot_product_distance(a, b),
    })
}
#[inline]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx512f") {
            return unsafe { cosine_distance_avx512(a, b) };
        }
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { cosine_distance_avx2(a, b) };
        }
    }
    cosine_distance_std_simd(a, b)
}
#[inline]
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx512f") {
            return unsafe { euclidean_distance_avx512(a, b) };
        }
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { euclidean_distance_avx2(a, b) };
        }
    }
    euclidean_distance_std_simd(a, b)
}
#[inline]
pub fn dot_product_distance(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if is_x86_feature_detected!("avx512f") {
            return unsafe { -dot_product_avx512(a, b) };
        }
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { -dot_product_avx2(a, b) };
        }
    }
    -dot_product_std_simd(a, b)
}
pub fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
pub fn euclidean_distance_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}
pub fn cosine_distance_scalar(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        1.0
    } else {
        1.0 - (dot / (na.sqrt() * nb.sqrt()))
    }
}
pub fn dot_product_std_simd(a: &[f32], b: &[f32]) -> f32 {
    let mut i = 0;
    let mut s = f32x8::splat(0.0);
    while i + 8 <= a.len() {
        s += f32x8::from_slice(&a[i..i + 8]) * f32x8::from_slice(&b[i..i + 8]);
        i += 8;
    }
    let mut r = s.reduce_sum();
    while i < a.len() {
        r += a[i] * b[i];
        i += 1;
    }
    r
}
pub fn euclidean_distance_std_simd(a: &[f32], b: &[f32]) -> f32 {
    let mut i = 0;
    let mut s = f32x8::splat(0.0);
    while i + 8 <= a.len() {
        let d = f32x8::from_slice(&a[i..i + 8]) - f32x8::from_slice(&b[i..i + 8]);
        s += d * d;
        i += 8;
    }
    let mut r = s.reduce_sum();
    while i < a.len() {
        let d = a[i] - b[i];
        r += d * d;
        i += 1;
    }
    r.sqrt()
}
pub fn cosine_distance_std_simd(a: &[f32], b: &[f32]) -> f32 {
    let mut i = 0;
    let (mut dot, mut na, mut nb) = (f32x8::splat(0.0), f32x8::splat(0.0), f32x8::splat(0.0));
    while i + 8 <= a.len() {
        let va = f32x8::from_slice(&a[i..i + 8]);
        let vb = f32x8::from_slice(&b[i..i + 8]);
        dot += va * vb;
        na += va * va;
        nb += vb * vb;
        i += 8;
    }
    let (mut fdot, mut fna, mut fnb) = (dot.reduce_sum(), na.reduce_sum(), nb.reduce_sum());
    while i < a.len() {
        fdot += a[i] * b[i];
        fna += a[i] * a[i];
        fnb += b[i] * b[i];
        i += 1;
    }
    if fna == 0.0 || fnb == 0.0 {
        1.0
    } else {
        1.0 - (fdot / (fna.sqrt() * fnb.sqrt()))
    }
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn hsum256_ps_avx(v: __m256) -> f32 {
    let x128 = _mm_add_ps(_mm256_extractf128_ps(v, 1), _mm256_castps256_ps128(v));
    let x64 = _mm_add_ps(x128, _mm_movehl_ps(x128, x128));
    let x32 = _mm_add_ss(x64, _mm_shuffle_ps(x64, x64, 0x55));
    _mm_cvtss_f32(x32)
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    let mut s = _mm256_setzero_ps();
    let mut i = 0;
    while i + 8 <= a.len() {
        s = _mm256_fmadd_ps(
            _mm256_loadu_ps(a.as_ptr().add(i)),
            _mm256_loadu_ps(b.as_ptr().add(i)),
            s,
        );
        i += 8;
    }
    let mut r = hsum256_ps_avx(s);
    while i < a.len() {
        r += a[i] * b[i];
        i += 1;
    }
    r
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn cosine_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (
        _mm256_setzero_ps(),
        _mm256_setzero_ps(),
        _mm256_setzero_ps(),
    );
    let mut i = 0;
    while i + 8 <= a.len() {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        d = _mm256_fmadd_ps(va, vb, d);
        na = _mm256_fmadd_ps(va, va, na);
        nb = _mm256_fmadd_ps(vb, vb, nb);
        i += 8;
    }
    let (mut fd, mut fna, mut fnb) = (hsum256_ps_avx(d), hsum256_ps_avx(na), hsum256_ps_avx(nb));
    while i < a.len() {
        fd += a[i] * b[i];
        fna += a[i] * a[i];
        fnb += b[i] * b[i];
        i += 1;
    }
    if fna == 0.0 || fnb == 0.0 {
        1.0
    } else {
        1.0 - (fd / (fna.sqrt() * fnb.sqrt()))
    }
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
unsafe fn euclidean_distance_avx2(a: &[f32], b: &[f32]) -> f32 {
    let mut s = _mm256_setzero_ps();
    let mut i = 0;
    while i + 8 <= a.len() {
        let diff = _mm256_sub_ps(
            _mm256_loadu_ps(a.as_ptr().add(i)),
            _mm256_loadu_ps(b.as_ptr().add(i)),
        );
        s = _mm256_fmadd_ps(diff, diff, s);
        i += 8;
    }
    let mut r = hsum256_ps_avx(s);
    while i < a.len() {
        let d = a[i] - b[i];
        r += d * d;
        i += 1;
    }
    r.sqrt()
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn hsum512_ps_avx(v: __m512) -> f32 {
    hsum256_ps_avx(_mm256_add_ps(
        _mm512_castps512_ps256(v),
        _mm512_extractf32x8_ps(v, 1),
    ))
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn dot_product_avx512(a: &[f32], b: &[f32]) -> f32 {
    let mut s = _mm512_setzero_ps();
    let mut i = 0;
    while i + 16 <= a.len() {
        s = _mm512_fmadd_ps(
            _mm512_loadu_ps(a.as_ptr().add(i)),
            _mm512_loadu_ps(b.as_ptr().add(i)),
            s,
        );
        i += 16;
    }
    let mut r = hsum512_ps_avx(s);
    while i < a.len() {
        r += a[i] * b[i];
        i += 1;
    }
    r
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn cosine_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (
        _mm512_setzero_ps(),
        _mm512_setzero_ps(),
        _mm512_setzero_ps(),
    );
    let mut i = 0;
    while i + 16 <= a.len() {
        let va = _mm512_loadu_ps(a.as_ptr().add(i));
        let vb = _mm512_loadu_ps(b.as_ptr().add(i));
        d = _mm512_fmadd_ps(va, vb, d);
        na = _mm512_fmadd_ps(va, va, na);
        nb = _mm512_fmadd_ps(vb, vb, nb);
        i += 16;
    }
    let (mut fd, mut fna, mut fnb) = (hsum512_ps_avx(d), hsum512_ps_avx(na), hsum512_ps_avx(nb));
    while i < a.len() {
        fd += a[i] * b[i];
        fna += a[i] * a[i];
        fnb += b[i] * b[i];
        i += 1;
    }
    if fna == 0.0 || fnb == 0.0 {
        1.0
    } else {
        1.0 - (fd / (fna.sqrt() * fnb.sqrt()))
    }
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f")]
unsafe fn euclidean_distance_avx512(a: &[f32], b: &[f32]) -> f32 {
    let mut s = _mm512_setzero_ps();
    let mut i = 0;
    while i + 16 <= a.len() {
        let diff = _mm512_sub_ps(
            _mm512_loadu_ps(a.as_ptr().add(i)),
            _mm512_loadu_ps(b.as_ptr().add(i)),
        );
        s = _mm512_fmadd_ps(diff, diff, s);
        i += 16;
    }
    let mut r = hsum512_ps_avx(s);
    while i < a.len() {
        let d = a[i] - b[i];
        r += d * d;
        i += 1;
    }
    r.sqrt()
}
pub fn normalize_inplace(v: &mut [f32]) {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v {
            *x /= n;
        }
    }
}
pub fn dot_product_u8(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| x as u32 * y as u32)
        .sum()
}
pub fn euclidean_distance_sq_u8(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let d = x as i32 - y as i32;
            (d * d) as u32
        })
        .sum()
}
#[derive(Debug, Clone, Copy)]
pub struct CosineSimilarityPartsU8 {
    pub dot: u32,
    pub sum_a: u32,
    pub sum_b: u32,
    pub norm_a_sq: u32,
    pub norm_b_sq: u32,
}
pub fn cosine_similarity_parts_u8(a: &[u8], b: &[u8]) -> CosineSimilarityPartsU8 {
    let (mut d, mut sa, mut sb, mut na, mut nb) = (0, 0, 0, 0, 0);
    for (&x, &y) in a.iter().zip(b.iter()) {
        let xu = x as u32;
        let yu = y as u32;
        d += xu * yu;
        sa += xu;
        sb += yu;
        na += xu * xu;
        nb += yu * yu;
    }
    CosineSimilarityPartsU8 {
        dot: d,
        sum_a: sa,
        sum_b: sb,
        norm_a_sq: na,
        norm_b_sq: nb,
    }
}
pub fn dot_product_f32_u8(a: &[f32], b: &[u8]) -> f32 {
    a.iter().zip(b.iter()).map(|(&x, &y)| x * (y as f32)).sum()
}
pub fn euclidean_distance_sq_f32_u8(a: &[f32], b: &[u8], alpha: f32, min: f32) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let yf = y as f32 * alpha + min;
            (x - yf) * (x - yf)
        })
        .sum()
}
#[derive(Debug, Clone, Copy)]
pub struct CosineSimilarityPartsF32U8 {
    pub dot_f32_u8: f32,
    pub sum_u8: u32,
    pub norm_u8_sq: u32,
}
pub fn cosine_similarity_parts_f32_u8(a: &[f32], b: &[u8]) -> CosineSimilarityPartsF32U8 {
    let mut d = 0.0;
    let mut s = 0;
    let mut n = 0;
    for (&x, &y) in a.iter().zip(b.iter()) {
        let yu = y as u32;
        d += x * (y as f32);
        s += yu;
        n += yu * yu;
    }
    CosineSimilarityPartsF32U8 {
        dot_f32_u8: d,
        sum_u8: s,
        norm_u8_sq: n,
    }
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
pub unsafe fn dot_product_f32_u8_avx2(a: &[f32], b: &[u8]) -> f32 {
    let n = a.len();
    let mut i = 0;
    let mut s = _mm256_setzero_ps();
    while i + 8 <= n {
        let vb_f = _mm256_cvtepi32_ps(_mm256_cvtepu8_epi32(_mm_loadl_epi64(
            b.as_ptr().add(i) as *const __m128i
        )));
        s = _mm256_fmadd_ps(_mm256_loadu_ps(a.as_ptr().add(i)), vb_f, s);
        i += 8;
    }
    let mut sum = hsum256_ps_avx(s);
    while i < n {
        sum += a[i] * (b[i] as f32);
        i += 1;
    }
    sum
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[target_feature(enable = "fma")]
pub unsafe fn euclidean_distance_sq_f32_u8_avx2(a: &[f32], b: &[u8], alpha: f32, min: f32) -> f32 {
    let n = a.len();
    let mut i = 0;
    let mut s = _mm256_setzero_ps();
    let av = _mm256_set1_ps(alpha);
    let mv = _mm256_set1_ps(min);
    while i + 8 <= n {
        let vb_f = _mm256_cvtepi32_ps(_mm256_cvtepu8_epi32(_mm_loadl_epi64(
            b.as_ptr().add(i) as *const __m128i
        )));
        let deq = _mm256_fmadd_ps(vb_f, av, mv);
        let diff = _mm256_sub_ps(_mm256_loadu_ps(a.as_ptr().add(i)), deq);
        s = _mm256_fmadd_ps(diff, diff, s);
        i += 8;
    }
    let mut sum = hsum256_ps_avx(s);
    while i < n {
        let yf = (b[i] as f32) * alpha + min;
        sum += (a[i] - yf) * (a[i] - yf);
        i += 1;
    }
    sum
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_dist() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((compute_distance(&a, &b, DistanceMetric::Cosine).unwrap() - 1.0).abs() < 1e-6);
    }
}
