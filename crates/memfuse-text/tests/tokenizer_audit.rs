// FILE-CONTEXT: Tokenizer Robustness & Property-Based Monotonicity Audit Suite.
// ZWECK: Verifiziert Fuzz-Robustheit (0 Panics), BM25-Score-Monotonie und Tokenisierungs-Grenzfälle.

use memfuse_text::bm25::score_term;
use memfuse_text::tokenizer::{DefaultTokenizer, GermanMorphTokenizer, Tokenizer};
use proptest::prelude::*;

proptest! {
    // 1. Property Test: DefaultTokenizer never panics on arbitrary Unicode inputs
    #[test]
    fn prop_default_tokenizer_no_panic(s in "\\PC*") {
        let tok = DefaultTokenizer;
        let tokens = tok.tokenize(&s);
        prop_assert!(tokens.len() <= s.len() + 1);
    }

    // 2. Property Test: GermanMorphTokenizer never panics on arbitrary Unicode inputs
    #[test]
    fn prop_german_morph_tokenizer_no_panic(s in "\\PC*") {
        let tok = GermanMorphTokenizer::new();
        let tokens = tok.tokenize(&s);
        let _ = tokens;
    }

    // 3. Property Test: BM25 score monotonicity w.r.t term frequency
    // Holding N, df, doc_len, avg_doc_len constant, increasing tf MUST NOT decrease the score
    #[test]
    fn prop_bm25_score_tf_monotonicity(
        tf1 in 1..500u32,
        tf_delta in 1..500u32,
        doc_len in 10..5000u32,
        avg_doc_len in 10.0..5000.0f32,
        df in 1..100u32,
        n in 100..10000u32,
    ) {
        let tf2 = tf1 + tf_delta;
        let score1 = score_term(tf1, doc_len, avg_doc_len, df, n);
        let score2 = score_term(tf2, doc_len, avg_doc_len, df, n);

        prop_assert!(
            score2 >= score1,
            "BM25 score must be monotonic non-decreasing with increasing TF (score1: {}, score2: {}, tf1: {}, tf2: {})",
            score1, score2, tf1, tf2
        );
    }
}

#[test]
fn test_tokenizer_robustness_edge_cases() {
    let default_tok = DefaultTokenizer;
    let german_tok = GermanMorphTokenizer::new();

    // 1. Empty string & whitespace
    assert!(default_tok.tokenize("").is_empty());
    assert!(default_tok.tokenize("   \t\n  ").is_empty());
    assert!(german_tok.tokenize("").is_empty());
    assert!(german_tok.tokenize("   \t\n  ").is_empty());

    // 2. Punctuation only
    assert!(default_tok.tokenize("...,,,!!!???---").is_empty());
    assert!(german_tok.tokenize("...,,,!!!???---").is_empty());

    // 3. Unicode & Emojis
    let emoji_text = "MemFuse 🚀 Such-Engine 🔥 mit 🦀 Rust";
    let default_emoji_tokens = default_tok.tokenize(emoji_text);
    let german_emoji_tokens = german_tok.tokenize(emoji_text);

    assert!(default_emoji_tokens.contains(&"memfuse".to_string()));
    assert!(default_emoji_tokens.contains(&"rust".to_string()));
    assert!(german_emoji_tokens.contains(&"memfuse".to_string()));

    // 4. Mixed CJK and German
    let cjk_text = "MemFuse Suche 検索 引擎 Textverarbeitung";
    let default_cjk_tokens = default_tok.tokenize(cjk_text);
    let german_cjk_tokens = german_tok.tokenize(cjk_text);

    assert!(default_cjk_tokens.contains(&"suche".to_string()));
    assert!(german_cjk_tokens.contains(&"suche".to_string()));

    // 5. Extremely long single word (>1000 chars)
    let long_word = "a".repeat(1500);
    let long_tokens = default_tok.tokenize(&long_word);
    assert_eq!(long_tokens.len(), 1);
    assert_eq!(long_tokens[0].len(), 1500);

    let german_long_tokens = german_tok.tokenize(&long_word);
    assert!(!german_long_tokens.is_empty());

    // 6. Numbers & German Decimals ("3,14" vs "3.14")
    let num_text = "Der Wert ist 3,14 oder 3.14 EUR";
    let default_num_tokens = default_tok.tokenize(num_text);
    let german_num_tokens = german_tok.tokenize(num_text);

    assert!(default_num_tokens.contains(&"wert".to_string()));
    assert!(default_num_tokens.contains(&"eur".to_string()));
    assert!(german_num_tokens.contains(&"wert".to_string()));

    // 7. URLs & Emails
    // Note: unicode_words() preserves dot-domain tokens as "memfuse.io" rather than splitting at the dot.
    let url_text = "Kontakt unter support@memfuse.io oder https://memfuse.io/docs";
    let default_url_tokens = default_tok.tokenize(url_text);
    let german_url_tokens = german_tok.tokenize(url_text);

    assert!(default_url_tokens.contains(&"support".to_string()), "default_url_tokens: {:?}", default_url_tokens);
    assert!(default_url_tokens.contains(&"memfuse.io".to_string()), "default_url_tokens: {:?}", default_url_tokens);
    assert!(german_url_tokens.contains(&"support".to_string()), "german_url_tokens: {:?}", german_url_tokens);
}
