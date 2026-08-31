// FILE-CONTEXT: German Morphology Audit & 45+ Compound Linguistic Ground-Truth Suite.
// ZWECK: Verifiziert Kompositazerlegung, Umlautnormalisierung, False-Positive-Rate & End-to-End Tokenisierung.

use memfuse_text::morphology::{
    normalize_umlauts, GermanCompoundSplitter, MorphologicalTokenizer,
};
use memfuse_text::tokenizer::{GermanMorphTokenizer, Tokenizer};

struct CompoundTestCase {
    word: &'static str,
    expected: &'static [&'static str],
    category: &'static str,
}

#[test]
fn test_german_compound_splitter_45_corpus() {
    let splitter = GermanCompoundSplitter::new();

    let corpus = [
        // 1. Fugen-s
        CompoundTestCase {
            word: "urlaubsantragsprozess",
            expected: &["urlaubs", "antrags", "prozess"],
            category: "Fugen-s (3-part KMU)",
        },
        CompoundTestCase {
            word: "arbeitsvertrag",
            expected: &["arbeits", "vertrag"],
            category: "Fugen-s",
        },
        CompoundTestCase {
            word: "auftragsbestaetigung",
            expected: &["auftrags", "bestaetigung"],
            category: "Fugen-s",
        },
        CompoundTestCase {
            word: "rechnungsbetrag",
            expected: &["rechnungs", "betrag"],
            category: "Fugen-s",
        },
        CompoundTestCase {
            word: "geschaeftsfuehrung",
            expected: &["geschaefts", "fuehrung"],
            category: "Fugen-s",
        },
        CompoundTestCase {
            word: "qualitaetspruefung",
            expected: &["qualitaets", "pruefung"],
            category: "Fugen-s",
        },
        CompoundTestCase {
            word: "versicherungsnetzwerk",
            expected: &["versicherungs", "netzwerk"],
            category: "Fugen-s",
        },
        CompoundTestCase {
            word: "entwicklungsumgebung",
            expected: &["entwicklungs", "umgebung"],
            category: "Fugen-s",
        },
        CompoundTestCase {
            word: "sicherheitsueberpruefung",
            expected: &["sicherheits", "ueberpruefung"],
            category: "Fugen-s",
        },
        CompoundTestCase {
            word: "beratungsgespraech",
            expected: &["beratungs", "gespraech"],
            category: "Fugen-s",
        },

        // 2. Fugen-en / Fugen-n
        CompoundTestCase {
            word: "blumenladen",
            expected: &["blumen", "laden"],
            category: "Fugen-n",
        },
        CompoundTestCase {
            word: "firmenleitung",
            expected: &["firmen", "leitung"],
            category: "Fugen-en",
        },
        CompoundTestCase {
            word: "kundenbetreuung",
            expected: &["kunden", "betreuung"],
            category: "Fugen-n",
        },
        CompoundTestCase {
            word: "expertenwissen",
            expected: &["experten", "wissen"],
            category: "Fugen-n",
        },
        CompoundTestCase {
            word: "lieferantenkatalog",
            expected: &["lieferanten", "katalog"],
            category: "Fugen-en",
        },
        CompoundTestCase {
            word: "strassenverkehr",
            expected: &["strassen", "verkehr"],
            category: "Fugen-n",
        },
        CompoundTestCase {
            word: "sonnenenergie",
            expected: &["sonnen", "energie"],
            category: "Fugen-n",
        },
        CompoundTestCase {
            word: "taschenrechner",
            expected: &["taschen", "rechner"],
            category: "Fugen-n",
        },

        // 3. Fugen-e
        CompoundTestCase {
            word: "hundehuette",
            expected: &["hunde", "huette"],
            category: "Fugen-e",
        },
        CompoundTestCase {
            word: "schweinebraten",
            expected: &["schweine", "braten"],
            category: "Fugen-e",
        },
        CompoundTestCase {
            word: "lesebuch",
            expected: &["lese", "buch"],
            category: "Fugen-e",
        },

        // 4. Fugen-er
        CompoundTestCase {
            word: "kinderbuch",
            expected: &["kinder", "buch"],
            category: "Fugen-er",
        },
        CompoundTestCase {
            word: "maennerchor",
            expected: &["maenner", "chor"],
            category: "Fugen-er",
        },
        CompoundTestCase {
            word: "bilderbuch",
            expected: &["bilder", "buch"],
            category: "Fugen-er",
        },
        CompoundTestCase {
            word: "woerterbuch",
            expected: &["woerter", "buch"],
            category: "Fugen-er",
        },

        // 5. Fugen-es
        CompoundTestCase {
            word: "tagesordnung",
            expected: &["tages", "ordnung"],
            category: "Fugen-es",
        },
        CompoundTestCase {
            word: "landesgericht",
            expected: &["landes", "gericht"],
            category: "Fugen-es",
        },

        // 6. Zero Interfix
        CompoundTestCase {
            word: "personalausweis",
            expected: &["personal", "ausweis"],
            category: "Zero interfix",
        },
        CompoundTestCase {
            word: "pflegeheim",
            expected: &["pflege", "heim"],
            category: "Zero interfix",
        },
        CompoundTestCase {
            word: "handtuch",
            expected: &["hand", "tuch"],
            category: "Zero interfix",
        },
        CompoundTestCase {
            word: "datenspeicher",
            expected: &["daten", "speicher"],
            category: "Zero interfix",
        },
        CompoundTestCase {
            word: "vektorsuche",
            expected: &["vektor", "suche"],
            category: "Zero interfix",
        },
        CompoundTestCase {
            word: "bilanzanalyse",
            expected: &["bilanz", "analyse"],
            category: "Zero interfix",
        },
        CompoundTestCase {
            word: "gesetzbuch",
            expected: &["gesetz", "buch"],
            category: "Zero interfix",
        },

        // 7. Multi-stem Enterprise Compounds (3-4 parts)
        CompoundTestCase {
            word: "bundesverfassungsgericht",
            expected: &["bundes", "verfassungs", "gericht"],
            category: "3-part",
        },
        CompoundTestCase {
            word: "hauptbahnhof",
            expected: &["haupt", "bahn", "hof"],
            category: "3-part",
        },
        CompoundTestCase {
            word: "lagerbestandsverwaltung",
            expected: &["lager", "bestands", "verwaltung"],
            category: "3-part",
        },
        CompoundTestCase {
            word: "lebensversicherungsgesellschaft",
            expected: &["lebens", "versicherungs", "gesellschaft"],
            category: "3-part KMU",
        },
        CompoundTestCase {
            word: "qualitaetsmanagementsystem",
            expected: &["qualitaets", "management", "system"],
            category: "3-part KMU",
        },
        CompoundTestCase {
            word: "datenschutzrichtlinie",
            expected: &["datenschutz", "richtlinie"],
            category: "KMU technical",
        },
        CompoundTestCase {
            word: "datenschutzerklaerung",
            expected: &["datenschutz", "erklaerung"],
            category: "KMU technical",
        },
        CompoundTestCase {
            word: "kraftfahrzeughaftpflichtversicherung",
            expected: &["kraft", "fahrzeug", "haftpflicht", "versicherung"],
            category: "4-part KMU extreme",
        },
        CompoundTestCase {
            word: "donaudampfschifffahrtsgesellschaftskapitaen",
            expected: &["donau", "dampf", "schifffahrts", "gesellschafts", "kapitaen"],
            category: "5-part extreme benchmark",
        },
        CompoundTestCase {
            word: "softwareentwicklungskontext",
            expected: &["software", "entwicklungs", "kontext"],
            category: "Loanword hybrid compound",
        },
        CompoundTestCase {
            word: "systemadministrator",
            expected: &["system", "administrator"],
            category: "IT compound",
        },
    ];

    let mut passed = 0;
    let total = corpus.len();

    println!("\n=== GERMAN COMPOUND SPLITTER EVALUATION ===");
    for tc in &corpus {
        let result = splitter.decompose(tc.word);
        let ok = result == tc.expected;
        if ok {
            passed += 1;
            println!(" [PASS] [{}] '{}' -> {:?}", tc.category, tc.word, result);
        } else {
            println!(
                " [FAIL] [{}] '{}': Expected {:?}, Got {:?}",
                tc.category, tc.word, tc.expected, result
            );
        }
    }

    let pass_rate = (passed as f64) / (total as f64) * 100.0;
    println!(
        "\nTotal Passed: {} / {} ({:.2}% Accuracy)",
        passed, total, pass_rate
    );

    assert!(
        pass_rate >= 90.0,
        "Morphology decomposition accuracy must be >= 90%, got {:.2}%",
        pass_rate
    );
}

#[test]
fn test_normalize_umlauts_full_coverage() {
    // Standard umlauts and sharp s
    assert_eq!(normalize_umlauts("Ärger"), "aerger");
    assert_eq!(normalize_umlauts("Ölpreis"), "oelpreis");
    assert_eq!(normalize_umlauts("Überwachung"), "ueberwachung");
    assert_eq!(normalize_umlauts("Straße"), "strasse");

    // Case variations
    assert_eq!(normalize_umlauts("äöüß"), "aeoeuess");
    assert_eq!(normalize_umlauts("ÄÖÜ"), "aeoeue");

    // Non-German ASCII text unchanged
    assert_eq!(normalize_umlauts("database"), "database");
}

#[test]
fn test_false_positive_loanword_behavior() {
    let splitter = GermanCompoundSplitter::new();

    // Loanwords/non-compound English terms should fall back to returning unsplit
    let non_compounds = ["marketing", "computer", "software", "manager", "cloud"];

    for word in &non_compounds {
        let parts = splitter.decompose(word);
        assert_eq!(
            parts,
            vec![*word],
            "Single stem / English loanword '{}' should not be falsely split into fragments",
            word
        );
    }
}

#[test]
fn test_german_morph_tokenizer_end_to_end_paragraph() {
    let tokenizer = GermanMorphTokenizer::new();

    let paragraph = "Der Urlaubsantragsprozess in der Lebensversicherungsgesellschaft \
                     erfordert eine Sicherheitsüberprüfung und Qualitätsprüfung gemäss Datenschutzrichtlinie.";

    let tokens = tokenizer.tokenize(paragraph);

    // Verify key compounds are decomposed for recall
    assert!(tokens.contains(&"urlaubsantragsprozess".to_string()));
    assert!(tokens.contains(&"prozess".to_string()));
    assert!(tokens.contains(&"lebensversicherungsgesellschaft".to_string()));
    assert!(tokens.contains(&"gesellschaft".to_string()));
    assert!(tokens.contains(&"sicherheitsueberpruefung".to_string()));
    assert!(tokens.contains(&"qualitaetspruefung".to_string()));
    assert!(tokens.contains(&"datenschutzrichtlinie".to_string()));

    // Verify stopwords are filtered
    assert!(!tokens.contains(&"der".to_string()));
    assert!(!tokens.contains(&"in".to_string()));
    assert!(!tokens.contains(&"eine".to_string()));
    assert!(!tokens.contains(&"und".to_string()));
}
