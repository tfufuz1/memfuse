use memfuse_core::EntityId;
use std::collections::HashSet;

/// Extrahiert einfache Entitäten aus Dokumenttext via Musterheuristiken.
/// Bewusst regelbasiert (keine ML-Abhängigkeit) für Nachvollziehbarkeit
/// und Zero-Setup-Betrieb.
///
/// # Extraktionsverhalten
/// - Erkennt aufeinanderfolgende großgeschriebene Wörter als Mehrwortphrasen (z.B. "Müller GmbH", "Max Mustermann", "Abteilung Finanzen").
/// - Erfordert Phrasenlänge >= 2 Wörter ODER ein erkanntes Unternehmenssuffix (z.B. "GmbH", "AG", "KG", "GbR", "OHG", "e.V.").
///
/// # Bekannte Einschränkungen (Known Limitations)
/// - Einwort-Eigennamen ohne Unternehmenssuffix (z.B. Städte- oder Ländernamen wie "Berlin", "Deutschland", "Paris")
///   werden derzeit NICHT als Entitäten extrahiert, da sie weder eine Mehrwortfolge bilden noch in der Suffix-Liste stehen.
pub struct SimpleEntityExtractor;

impl SimpleEntityExtractor {
    /// Erkennt großgeschriebene Mehrwortfolgen als potenzielle Eigennamen
    /// (Personen, Firmen, Abteilungen) — deutsche Substantiv-Großschreibung
    /// macht dies überraschend robust für Geschäftsdokumente.
    pub fn extract(text: &str) -> Vec<EntityId> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let mut entities = HashSet::new();

        // Muster 1: Aufeinanderfolgende großgeschriebene Wörter
        // (z.B. "Müller GmbH", "Max Mustermann", "Abteilung Finanzen")
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut i = 0;
        while i < words.len() {
            let word = words[i].trim_matches(|c: char| !c.is_alphanumeric());
            if Self::is_capitalized_candidate(word) {
                let mut phrase = vec![word];
                let mut j = i + 1;
                while j < words.len() {
                    let next = words[j].trim_matches(|c: char| !c.is_alphanumeric());
                    if Self::is_capitalized_candidate(next) {
                        phrase.push(next);
                        j += 1;
                    } else {
                        break;
                    }
                }
                if phrase.len() >= 2 || Self::looks_like_company_suffix(word) {
                    let joined = phrase.join(" ");
                    if joined.len() > 3 {
                        entities.insert(EntityId::from(joined.as_str()));
                    }
                }
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }

        entities.into_iter().collect()
    }

    fn is_capitalized_candidate(word: &str) -> bool {
        word.chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
            && word.len() > 2
            && !Self::is_common_sentence_starter(word)
    }

    /// Filtert häufige Satzanfangs-Wörter heraus, die großgeschrieben sind,
    /// aber keine Entitäten darstellen (jedes Satzanfangs-Wort ist im
    /// Deutschen großgeschrieben, unabhängig vom Wortstamm).
    fn is_common_sentence_starter(word: &str) -> bool {
        matches!(
            word,
            "Der"
                | "Die"
                | "Das"
                | "Ein"
                | "Eine"
                | "Wir"
                | "Sie"
                | "Ihr"
                | "Bitte"
                | "Für"
                | "Diese"
                | "Dieser"
                | "Alle"
                | "Jeder"
        )
    }

    fn looks_like_company_suffix(word: &str) -> bool {
        matches!(word, "GmbH" | "AG" | "KG" | "GbR" | "OHG" | "e.V.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_company_name() {
        let text = "Müller GmbH hat eine Anfrage gestellt.";
        let entities = SimpleEntityExtractor::extract(text);
        assert!(!entities.is_empty());
        let expected = EntityId::from("Müller GmbH");
        assert!(entities.contains(&expected));
    }

    #[test]
    fn test_extract_multiple_entities() {
        let text = "Max Mustermann arbeitet in der Abteilung Finanzen bei der Acme Corp AG.";
        let entities = SimpleEntityExtractor::extract(text);
        let muller = EntityId::from("Max Mustermann");
        let finance = EntityId::from("Abteilung Finanzen");
        assert!(entities.contains(&muller));
        assert!(entities.contains(&finance));
    }

    #[test]
    fn test_sentence_starter_filtered() {
        let text = "Diese Anfrage wurde von der Firma verarbeitet.";
        let entities = SimpleEntityExtractor::extract(text);
        let starter = EntityId::from("Diese Anfrage");
        assert!(!entities.contains(&starter));
    }

    #[test]
    fn test_empty_input_returns_empty_vec() {
        let entities = SimpleEntityExtractor::extract("");
        assert!(entities.is_empty());
    }

    #[test]
    fn test_no_entities_found_returns_empty_vec() {
        let text = "hier ist nur kleingeschriebener text ohne namen";
        let entities = SimpleEntityExtractor::extract(text);
        assert!(entities.is_empty());
    }
}
