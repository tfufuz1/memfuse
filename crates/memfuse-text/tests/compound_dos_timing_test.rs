//! DoS-Timing-Test für GermanCompoundSplitter::decompose().
//! Sichert: Kein exponentielles Backtracking bei langen Eingaben.
//! Referenz: MemFuse Round-2-Audit-Report (exponentielle Kompositazerlegung).

use memfuse_text::morphology::{GermanCompoundSplitter, MorphologicalTokenizer};
use std::time::{Duration, Instant};

const MAX_MS_SHORT: u64 = 100; // <= 200 Zeichen: max 100ms
const MAX_MS_LONG: u64 = 500; // 1000 Zeichen: max 500ms

fn time_decompose(splitter: &GermanCompoundSplitter, word: &str) -> Duration {
    let start = Instant::now();
    let _ = splitter.decompose(word);
    start.elapsed()
}

#[test]
fn test_real_long_compounds_terminate_fast() {
    let splitter = GermanCompoundSplitter::new();
    let cases = [
        "donaudampfschifffahrtsgesellschaftskapitaen",
        "kraftfahrzeughaftpflichtversicherungsgesellschaft",
        "rindfleischetikettierungsueberwachungsaufgabenuebertragungsgesetz",
        "urlaubsantragsgenehmigungsbearbeitungsprozessoptimierung",
    ];
    for word in &cases {
        let elapsed = time_decompose(&splitter, word);
        assert!(
            elapsed < Duration::from_millis(MAX_MS_SHORT),
            "decompose('{}') took {:?}, expected < {}ms",
            word,
            elapsed,
            MAX_MS_SHORT
        );
    }
}

#[test]
fn test_synthetic_length_scaling() {
    let splitter = GermanCompoundSplitter::new();
    // Synthetisch konstruierte Tokens steigender Länge
    // (real aussehende deutsche Wörter, keine reinen 'aaa'-Strings
    //  da Splitter echte Morphologie erwartet)
    let prefix = "arbeitsvertragsverlängerungs";
    for &repeat in &[1usize, 2, 3, 5] {
        let word = prefix.repeat(repeat);
        let elapsed = time_decompose(&splitter, &word);
        // Jede Variante muss unter MAX_MS_SHORT bleiben
        assert!(
            elapsed < Duration::from_millis(MAX_MS_SHORT),
            "decompose(len={}) took {:?}",
            word.len(),
            elapsed
        );
    }
}

#[test]
fn test_thousand_char_input_terminates() {
    let splitter = GermanCompoundSplitter::new();
    // 1000-Zeichen-Input (übergroß — Splitter hat Early-Exit für len > 200)
    let oversized = "a".repeat(1000);
    let elapsed = time_decompose(&splitter, &oversized);
    assert!(
        elapsed < Duration::from_millis(MAX_MS_LONG),
        "1000-char input took {:?}, expected < {}ms",
        elapsed,
        MAX_MS_LONG
    );
}

#[test]
fn test_decompose_is_deterministic() {
    // Gleiche Eingabe → gleiche Ausgabe (kein zufäliges Backtracking-Ergebnis)
    let splitter = GermanCompoundSplitter::new();
    let word = "qualitaetsmanagementsystem";
    let result1 = splitter.decompose(word);
    let result2 = splitter.decompose(word);
    assert_eq!(result1, result2, "decompose() must be deterministic");
}
