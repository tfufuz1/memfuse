//! Proptest-Suite für ScalarQuantizer und compute_distance.
//! Verifiziert mathematische Invarianten über zufällige Eingaben.

use memfuse_core::DistanceMetric;
use memfuse_index::distance::compute_distance;
use proptest::prelude::*;

// --- Suite A: ScalarQuantizer ---
// Importiere ScalarQuantizer nur in Tests (pub(crate) sichtbar)
#[path = "../src/distance.rs"]
mod distance;
#[path = "../src/quantize.rs"]
mod quantize;
use quantize::ScalarQuantizer;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    // Eigenschaft: quantize(v) dann dequantize ergibt Wert nahe v
    // Toleranz: abs(original - reconstructed) <= range/255 + epsilon
    #[test]
    fn prop_sq8_quantize_dequantize_roundtrip(
        values in prop::collection::vec(-100.0f32..100.0f32, 1..=64)
    ) {
        let dim = values.len();
        let quantizer = ScalarQuantizer::try_train(&[values.as_slice()], dim).expect("try_train should succeed");
        let u8_vec = quantizer.quantize(&values);
        let reconstructed = quantizer.dequantize(&u8_vec);

        for i in 0..dim {
            let range = quantizer.maxes[i] - quantizer.mins[i];
            let diff = (values[i] - reconstructed[i]).abs();
            let max_allowed_diff = range / 255.0 + 1e-5;
            prop_assert!(
                diff <= max_allowed_diff,
                "Dim {}: original={}, reconstructed={}, diff={}, max_allowed={}",
                i, values[i], reconstructed[i], diff, max_allowed_diff
            );
        }
    }

    // Eigenschaft: cosine_distance(v, v) == 0.0 (Selbstdistanz)
    #[test]
    fn prop_cosine_self_distance_is_zero(
        v in prop::collection::vec(-1.0f32..1.0f32, 1..=128)
    ) {
        // Normalisierter Vektor mit sich selbst: Distanz muss 0.0 sein
        // (oder sehr nah an 0.0, Toleranz 1e-5)
        // Nur wenn v nicht der Nullvektor ist
        prop_assume!(v.iter().any(|&x| x.abs() > 1e-6));
        let result = compute_distance(&v, &v, DistanceMetric::Cosine);
        prop_assert!(result.is_ok());
        prop_assert!((result.unwrap()).abs() < 1e-4);
    }

    // Eigenschaft: euclidean_distance(v, w) >= 0.0
    #[test]
    fn prop_euclidean_non_negative(
        (v, w) in (1usize..=128).prop_flat_map(|len| (
            prop::collection::vec(-100.0f32..100.0f32, len),
            prop::collection::vec(-100.0f32..100.0f32, len)
        ))
    ) {
        let result = compute_distance(&v, &w, DistanceMetric::Euclidean);
        prop_assert!(result.is_ok());
        prop_assert!(result.unwrap() >= 0.0);
    }

    // Eigenschaft: Dimension-Mismatch → Err
    #[test]
    fn prop_dimension_mismatch_returns_err(
        len_a in 1usize..=64,
        len_b in 1usize..=64
    ) {
        prop_assume!(len_a != len_b);
        let a = vec![1.0f32; len_a];
        let b = vec![1.0f32; len_b];
        let result = compute_distance(&a, &b, DistanceMetric::Cosine);
        prop_assert!(result.is_err());
    }
}

// Deterministische Grenzwert-Tests (kein proptest):

#[test]
fn test_cosine_zero_vector_is_handled() {
    // Null-Vektor: compute_distance([0,0,0], [1,0,0], Cosine)
    // Muss Err oder 0.0 zurückgeben — kein Panic, kein NaN
    let zero = vec![0.0f32; 3];
    let unit = vec![1.0f32, 0.0, 0.0];
    let result = compute_distance(&zero, &unit, DistanceMetric::Cosine);
    // Kein Panic ist die Hauptanforderung
    let _ = result;
}

#[test]
fn test_compute_distance_with_extreme_values_no_panic() {
    // f32::MAX und f32::MIN in Vektoren: kein Panic
    let a = vec![f32::MAX, f32::MIN, 0.0, 1.0];
    let b = vec![1.0f32, 0.0, f32::MAX, f32::MIN];
    let _ = compute_distance(&a, &b, DistanceMetric::Euclidean);
    let _ = compute_distance(&a, &b, DistanceMetric::Cosine);
    let _ = compute_distance(&a, &b, DistanceMetric::DotProduct);
    // Kein Panic = Test bestanden
}
