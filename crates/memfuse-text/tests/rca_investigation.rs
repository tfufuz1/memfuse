use memfuse_text::morphology::{GermanCompoundSplitter, MorphologicalTokenizer};
use std::collections::HashSet;
use std::time::Instant;

#[test]
fn test_rca_investigation_full_suite() {
    println!("\n=== RUND 2 RCA INVESTIGATION: GermanCompoundSplitter ===");

    // -------------------------------------------------------------
    // Task 2: Instrumenting the 3 Known Failures
    // -------------------------------------------------------------
    println!("\n--- TASK 2: Test of 3 Known Failures ---");

    let default_splitter = GermanCompoundSplitter::new();

    let failures = [
        ("donaudampfschifffahrtsgesellschaftskapitaen", 45),
        ("softwareentwicklungskontext", 27),
        ("systemadministrator", 19),
    ];

    for (word, len) in &failures {
        let start = Instant::now();
        let parts = default_splitter.decompose(word);
        let elapsed = start.elapsed();
        println!(
            "Word: '{}' (len {}): parts = {:?} | elapsed = {:?}",
            word, len, parts, elapsed
        );
    }

    // Now test with custom dictionary that includes missing stems
    let mut custom_dict = HashSet::new();
    // Add embedded default dictionary words if possible, plus missing stems
    let default_words = include_str!("../src/data/german_words.txt");
    for line in default_words.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            custom_dict.insert(trimmed.to_string());
        }
    }

    // Add missing stems from the 3 failure cases
    custom_dict.insert("donau".to_string());
    custom_dict.insert("dampf".to_string());
    custom_dict.insert("kapitaen".to_string());
    custom_dict.insert("kapitaen".to_string());
    custom_dict.insert("kontext".to_string());
    custom_dict.insert("administrator".to_string());
    custom_dict.insert("software".to_string());
    custom_dict.insert("entwicklung".to_string());
    custom_dict.insert("system".to_string());
    custom_dict.insert("gesellschaft".to_string());
    custom_dict.insert("schiff".to_string());
    custom_dict.insert("fahrt".to_string());

    let custom_splitter = GermanCompoundSplitter::with_dictionary(3, custom_dict);

    println!("\n--- TASK 2 (with missing dictionary stems added): ---");
    for (word, len) in &failures {
        let start = Instant::now();
        let parts = custom_splitter.decompose(word);
        let elapsed = start.elapsed();
        println!(
            "Word with dict additions: '{}' (len {}): parts = {:?} | elapsed = {:?}",
            word, len, parts, elapsed
        );
    }

    // -------------------------------------------------------------
    // Task 3 & 4: Threshold Determination & Latency Measurement
    // -------------------------------------------------------------
    println!("\n--- TASK 3 & 4: Length vs Subword Thresholds & Latency ---");

    // Construct synthetic words of increasing lengths with guaranteed dictionary words:
    // Stems: "system" (6), "kunden" (6), "daten" (5), "lager" (5), "betrieb" (7), "dienst" (6), "struktur" (8), "verwaltung" (10)
    let components = [
        "system",
        "kunden",
        "daten",
        "lager",
        "betrieb",
        "dienst",
        "struktur",
        "verwaltung",
    ];

    // Build synthetic compounds of varying component count
    let mut test_words = Vec::new();
    for count in 2..=25 {
        let mut word = String::new();
        for i in 0..count {
            word.push_str(components[i % components.len()]);
            if i < count - 1 && i % 2 == 1 {
                word.push('s'); // insert interfix -s- periodically
            }
        }
        test_words.push((count, word));
    }

    println!(
        "{:<10} | {:<10} | {:<10} | {:<20} | {:<15}",
        "Parts Count", "Byte Len", "Success?", "Sample Output", "Latency (µs)"
    );
    println!("{:-<75}", "");

    for (part_count, word) in &test_words {
        let start = Instant::now();
        let parts = custom_splitter.decompose(word);
        let elapsed = start.elapsed();

        let is_success = parts.len() > 1;
        let sample = if parts.len() > 3 {
            format!("[{}, {}, ... +{}]", parts[0], parts[1], parts.len() - 2)
        } else {
            format!("{:?}", parts)
        };

        println!(
            "{:<10} | {:<10} | {:<10} | {:<20} | {:<15.3}",
            part_count,
            word.len(),
            if is_success { "PASS" } else { "FAIL (unsplit)" },
            sample,
            elapsed.as_secs_f64() * 1_000_000.0
        );
    }

    // -------------------------------------------------------------
    // Task 5: DoS Test Case (200+ characters)
    // -------------------------------------------------------------
    println!("\n--- TASK 5: DoS Test Case (200+ characters) ---");

    // Construct a high-ambiguity pseudo-word with 200+ chars
    let repeat_stem = "land"; // "land" is in dictionary
    let dos_word_120 = repeat_stem.repeat(30); // 120 chars
    let dos_word_127 = repeat_stem.repeat(31) + "lan"; // 127 chars
    let dos_word_128 = repeat_stem.repeat(32); // 128 chars
    let dos_word_129 = repeat_stem.repeat(32) + "a"; // 129 chars
    let dos_word_200 = repeat_stem.repeat(50); // 200 chars
    let dos_word_500 = repeat_stem.repeat(125); // 500 chars

    let dos_inputs = [
        ("120 chars", dos_word_120.as_str()),
        ("127 chars", dos_word_127.as_str()),
        ("128 chars", dos_word_128.as_str()),
        ("129 chars", dos_word_129.as_str()),
        ("200 chars", dos_word_200.as_str()),
        ("500 chars", dos_word_500.as_str()),
    ];

    println!(
        "{:<15} | {:<10} | {:<10} | {:<15}",
        "Label", "Byte Len", "Parts Count", "Latency (µs)"
    );
    println!("{:-<60}", "");

    for (label, word) in &dos_inputs {
        let start = Instant::now();
        let parts = custom_splitter.decompose(word);
        let elapsed = start.elapsed();

        println!(
            "{:<15} | {:<10} | {:<10} | {:<15.3}",
            label,
            word.len(),
            parts.len(),
            elapsed.as_secs_f64() * 1_000_000.0
        );
    }
}
