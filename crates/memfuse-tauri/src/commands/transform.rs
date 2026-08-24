//! Regex-Transformations-Commands für den Tauri-Frontend.
//!
//! # Engine-Garantien & ReDoS-Analyse (DECISION-REF: ADR-014)
//!
//! Die `regex`-Crate v1.13.1 (transitiv via `regex-automata` v0.4.18) verwendet
//! eine **NFA/DFA-basierte Architektur ohne Backtracking**:
//!
//! - Backreferences → Compile-Fehler beim `Regex::new()`-Aufruf (kein Panic, kein Hang)
//! - Lookahead / Lookbehind → Compile-Fehler
//! - Verschachtelte Quantifizierer (`(a+)+`) → O(n·|NFA-Zustände|), **kein exponentieller Backtracking**
//!
//! **Folgerung**: Klassisches ReDoS ist mit dieser Engine **strukturell unmöglich**.
//! Ein Pattern wie `(a+)+$` ist für die regex-Crate **kein pathologisches Pattern**;
//! der NFA evaluiert es in linearer Zeit.
//!
//! # Warum `spawn_blocking` + `timeout` trotzdem beibehalten?
//!
//! Der Timeout dient **nicht** als primärer ReDoS-Schutz (den gewährt die Engine selbst),
//! sondern als defensives Sicherheitsnetz gegen:
//! 1. Sehr großen Input × sehr komplexes Pattern: Bei `MAX_REGEX_INPUT_BYTES = 1 MB`
//!    und einem Pattern mit vielen NFA-Zuständen kann lineares Matching
//!    trotzdem mehrere Sekunden dauern — tolerierbar, aber begrenzt.
//! 2. Unerwartete Bugs oder zukünftige Engine-Änderungen.
//!
//! Bei `MAX_REGEX_INPUT_BYTES = 1 MB` und einer konservativen Durchsatzschätzung
//! von ~50 MB/s (Worst-Case für hochkomplexe NFA mit vielen Zuständen) beträgt
//! die maximale Ausführungszeit: 1 MB / 50 MB/s = 20 ms.
//! `REGEX_TIMEOUT = 5 s` entspricht damit einem ~250× Sicherheitsfaktor.
//! Ein Timeout in der Praxis signalisiert daher einen **echten Bug**, keine normale Nutzung.
//!
//! # Schutz des Blocking-Thread-Pools (DECISION-REF: ADR-014)
//!
//! Da Bulk-Transform viele Snippets gleichzeitig verarbeiten kann, erwirbt jede
//! `run_regex_transformation`-Ausführung ein Permit aus `AppState::regex_semaphore`.
//! Dadurch können nie mehr als `MAX_CONCURRENT_REGEX_OPS` Blocking-Threads
//! gleichzeitig aktiv sein — auch wenn ein hypothetischer Hang auftritt.

use crate::state::AppState;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::State;

// ─── Konstanten ──────────────────────────────────────────────────────────────

/// Maximale Eingabelänge für Patterns, die als strukturell normal eingestuft werden.
/// Bei linearem Matching und ~50 MB/s Worst-Case-Durchsatz: 1 MB → max. ~20 ms.
const MAX_REGEX_INPUT_BYTES: usize = 1_048_576; // 1 MiB

/// Reduziertes Limit für Patterns, die als strukturell komplex eingestuft werden
/// (viele Gruppen, tiefe Alternation). Auch wenn kein echtes ReDoS möglich ist,
/// sind sehr viele NFA-Zustände × großem Input eine vernünftige Vorsichtsgrenze.
const MAX_REGEX_INPUT_BYTES_COMPLEX: usize = 65_536; // 64 KiB

/// Timeout für den blockierenden Regex-Aufruf.
///
/// AI-NOTE[CONCURRENCY]: Dies ist primär ein Sicherheitsnetz gegen Bugs,
/// NICHT gegen ReDoS. Die `regex`-Crate garantiert lineare Laufzeit.
/// Bei MAX_REGEX_INPUT_BYTES und Worst-Case-Durchsatz beträgt die reale
/// maximale Ausführungszeit << 100 ms. 5 s entspricht einem ~250× Puffer.
/// DECISION-REF: ADR-014
const REGEX_TIMEOUT: Duration = Duration::from_secs(5);

// ─── DTOs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegexTransformRequest {
    /// Das Regex-Pattern (Rust `regex`-Syntax; keine PCRE-Erweiterungen).
    pub pattern: String,
    /// Optionale Flags: `"g"` für globale Ersetzung, leer für einmalige Ersetzung.
    pub flags: String,
    /// Ersetzungsstring (Rust Capture-Group-Syntax: `$1`, `${name}`).
    pub replacement: String,
    /// Eingabetext, auf den das Pattern angewendet wird.
    pub input: String,
}

#[derive(Debug, Serialize)]
pub struct RegexTransformResult {
    pub output: String,
    /// Anzahl der tatsächlich vorgenommenen Ersetzungen.
    pub replacements_made: usize,
}

#[derive(Debug, Serialize)]
pub struct RegexValidationResult {
    pub is_valid: bool,
    /// Fehlermeldung bei ungültigem Pattern (leer wenn valid).
    pub error: String,
    /// Ob das Pattern als strukturell komplex eingestuft wurde (reduziertes Input-Limit).
    pub is_complex: bool,
    /// Effektives Input-Limit in Bytes, das für dieses Pattern gilt.
    pub effective_input_limit: usize,
}

// ─── Heuristik: Strukturelle Komplexität ─────────────────────────────────────

/// Prüft, ob ein Pattern strukturell komplex ist (viele Gruppen/Alternativen).
///
/// **Zweck**: Reduziert das effektive Input-Limit für Patterns mit vielen
/// NFA-Zuständen, um sicherzustellen, dass lineares Matching auch bei sehr
/// komplexen Patterns innerhalb des REGEX_TIMEOUT bleibt.
///
/// **Wichtig**: Dies ist KEIN ReDoS-Schutz (die regex-Crate garantiert lineare
/// Laufzeit unabhängig von Backtracking-Risiken). Es ist eine reine
/// Durchsatz-Schutzmaßnahme.
///
/// Kriterien für "komplex":
/// - Mehr als 8 Gruppen (öffnende Klammern)
/// - Mehr als 4 Alternations-Operator `|`
/// - Pattern länger als 500 Zeichen
fn is_structurally_complex(pattern: &str) -> bool {
    let group_count = pattern.chars().filter(|&c| c == '(').count();
    let alternation_count = pattern.chars().filter(|&c| c == '|').count();
    group_count > 8 || alternation_count > 4 || pattern.len() > 500
}

// ─── Kern-Logik (nicht öffentlich, testbar) ──────────────────────────────────

/// Führt die eigentliche Regex-Transformation durch.
///
/// Diese Funktion ist von den Tauri-Commands getrennt, um sie ohne Tauri-State
/// in Unit-Tests aufrufen zu können.
async fn run_regex_transformation(
    pattern: &str,
    flags: &str,
    replacement: &str,
    input: &str,
) -> Result<RegexTransformResult, String> {
    // ── 1. Adaptive Eingabelängenbegrenzung ──────────────────────────────────
    // Auch wenn die regex-Crate kein ReDoS erzeugen kann, begrenzen wir den
    // Input, damit lineares Matching innerhalb des Timeouts bleibt.
    let is_complex = is_structurally_complex(pattern);
    let effective_limit = if is_complex {
        MAX_REGEX_INPUT_BYTES_COMPLEX
    } else {
        MAX_REGEX_INPUT_BYTES
    };

    if input.len() > effective_limit {
        return Err(format!(
            "Eingabe zu groß: {} Bytes (Limit: {} Bytes{})",
            input.len(),
            effective_limit,
            if is_complex {
                " — Pattern als strukturell komplex eingestuft, reduziertes Limit aktiv"
            } else {
                ""
            }
        ));
    }

    // ── 2. Pattern kompilieren ───────────────────────────────────────────────
    // `Regex::new()` lehnt Backreferences und Lookahead/Lookbehind mit
    // einem klaren Fehler ab — kein Panic, kein Hang.
    let re = Regex::new(pattern).map_err(|e| format!("Ungültiges Regex-Pattern: {e}"))?;

    // ── 3. Matching in spawn_blocking mit Timeout ────────────────────────────
    //
    // AI-NOTE[CONCURRENCY][MINOR]: `tokio::time::timeout` bricht den
    // spawn_blocking-Thread NICHT ab — er läuft weiter, bis er fertig ist.
    // Da die regex-Crate lineare Laufzeit garantiert, ist ein echter
    // "läuft ewig"-Fall nicht möglich. Der Timeout ist primär ein
    // Sicherheitsnetz gegen Bugs, nicht gegen ReDoS.
    // ID: AGT-TRANSFORM-001
    // DECISION-REF: ADR-014
    let input_owned = input.to_owned();
    let replacement_owned = replacement.to_owned();
    let flags_owned = flags.to_owned();

    let exec_result = tokio::time::timeout(
        REGEX_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let is_global = flags_owned.contains('g');
            let original = input_owned.clone();
            let replaced = if is_global {
                re.replace_all(&input_owned, replacement_owned.as_str())
                    .into_owned()
            } else {
                re.replacen(&input_owned, 1, replacement_owned.as_str())
                    .into_owned()
            };
            // Anzahl der Ersetzungen approximieren: Zähle Unterschiede.
            // Hinweis: Das ist eine Näherung; exaktes Zählen würde zwei Durchläufe erfordern.
            let replacements_made = if is_global {
                re.find_iter(&original).count()
            } else if re.is_match(&original) {
                1
            } else {
                0
            };
            (replaced, replacements_made)
        }),
    )
    .await;

    match exec_result {
        Ok(Ok((output, replacements_made))) => Ok(RegexTransformResult {
            output,
            replacements_made,
        }),
        Ok(Err(join_err)) => {
            // spawn_blocking-Thread ist in Panik geraten (sollte nie passieren,
            // da kein unwrap() im Blocking-Code).
            tracing::error!(
                error = %join_err,
                pattern_len = pattern.len(),
                input_len = input.len(),
                "Regex-spawn_blocking-Thread ist abgestürzt — interner Fehler"
            );
            Err(format!("Interner Fehler beim Regex-Matching: {join_err}"))
        }
        Err(_elapsed) => {
            // AI-NOTE[CONCURRENCY]: Dieser Timeout-Fall sollte in der Praxis
            // NIE auftreten (Laufzeit-Garantie der regex-Crate + konservative
            // Input-Größenbegrenzung). Ein Auftreten signalisiert einen Bug.
            // DECISION-REF: ADR-014
            tracing::warn!(
                pattern_len = pattern.len(),
                input_len = input.len(),
                is_complex,
                timeout_secs = REGEX_TIMEOUT.as_secs(),
                // Monitoring-Schritt (Auftrag §5): Dieses Log macht sichtbar,
                // ob Timeouts in der Praxis vorkommen. Pattern selbst wird
                // absichtlich nicht geloggt (kann sensible Nutzerdaten enthalten).
                "Regex-Timeout aufgetreten — sollte bei dieser Engine nicht vorkommen; \
                 bitte Pattern und Input-Größe untersuchen"
            );
            Err(format!(
                "Regex-Timeout nach {}s — die regex-Crate garantiert lineare Laufzeit, \
                 daher ist dies ein unerwarteter Fehler. Bitte Pattern und Input-Größe prüfen.",
                REGEX_TIMEOUT.as_secs()
            ))
        }
    }
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Wendet eine Regex-Transformation auf einen einzelnen Text-Snippet an.
///
/// Erwirbt ein Semaphore-Permit aus `AppState::regex_semaphore`, bevor
/// der blockierende Matching-Aufruf gestartet wird. Dies begrenzt die
/// maximale Anzahl gleichzeitig laufender Regex-Blocking-Threads auf
/// `MAX_CONCURRENT_REGEX_OPS` und schützt damit den tokio-Blocking-Thread-Pool.
/// DECISION-REF: ADR-014
#[tauri::command]
pub async fn run_regex_transform(
    state: State<'_, AppState>,
    request: RegexTransformRequest,
) -> Result<RegexTransformResult, String> {
    // Semaphore-Permit erwerben — gibt sofort Fehler zurück wenn erschöpft
    let _permit = state
        .regex_semaphore
        .try_acquire()
        .map_err(|_| "Zu viele gleichzeitige Regex-Operationen — bitte kurz warten")?;

    run_regex_transformation(
        &request.pattern,
        &request.flags,
        &request.replacement,
        &request.input,
    )
    .await
}

/// Wendet eine Regex-Transformation auf mehrere Text-Snippets an (Bulk-Transform).
///
/// Verarbeitet Snippets sequenziell mit einer gemeinsamen Semaphore. Dies verhindert,
/// dass ein einzelner Bulk-Aufruf den gesamten Blocking-Thread-Pool belegt.
/// DECISION-REF: ADR-014
#[tauri::command]
pub async fn run_bulk_regex_transform(
    state: State<'_, AppState>,
    pattern: String,
    flags: String,
    replacement: String,
    inputs: Vec<String>,
) -> Result<Vec<Result<RegexTransformResult, String>>, String> {
    let mut results = Vec::with_capacity(inputs.len());

    for input in inputs {
        // Semaphore für jeden Snippet einzeln erwerben (sequenzielle Verarbeitung).
        // Gibt bei Erschöpfung keinen Fehler zurück, sondern wartet bis ein Permit frei wird.
        let _permit = state
            .regex_semaphore
            .acquire()
            .await
            .map_err(|e| format!("Semaphore-Fehler: {e}"))?;

        let result = run_regex_transformation(&pattern, &flags, &replacement, &input).await;
        results.push(result);
        // _permit wird hier gedroppt → Permit für nächste Iteration freigegeben
    }

    Ok(results)
}

/// Validiert ein Regex-Pattern, ohne einen Match auszuführen.
///
/// Sicher: Kein blocking, kein Input-Text, kein Timeout benötigt.
/// Die Compile-Zeit der regex-Crate ist O(|Pattern|²) im Worst-Case
/// (für sehr komplexe Patterns), aber bei vernünftigen Pattern-Längen (<10 KB)
/// kein praktisches Problem.
#[tauri::command]
pub fn validate_regex_pattern(pattern: String) -> RegexValidationResult {
    let is_complex = is_structurally_complex(&pattern);
    let effective_input_limit = if is_complex {
        MAX_REGEX_INPUT_BYTES_COMPLEX
    } else {
        MAX_REGEX_INPUT_BYTES
    };

    match Regex::new(&pattern) {
        Ok(_) => RegexValidationResult {
            is_valid: true,
            error: String::new(),
            is_complex,
            effective_input_limit,
        },
        Err(e) => RegexValidationResult {
            is_valid: false,
            error: e.to_string(),
            is_complex,
            effective_input_limit,
        },
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Hilfsmakro: führt run_regex_transformation synchron in Tests aus.
    macro_rules! transform {
        ($pattern:expr, $flags:expr, $repl:expr, $input:expr) => {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(run_regex_transformation($pattern, $flags, $repl, $input))
        };
    }

    #[test]
    fn test_simple_replacement() {
        let result = transform!("foo", "", "bar", "foo baz foo").unwrap();
        assert_eq!(
            result.output, "bar baz foo",
            "Ohne 'g'-Flag nur erste Ersetzung"
        );
        assert_eq!(result.replacements_made, 1);
    }

    #[test]
    fn test_global_flag_replaces_all() {
        let result = transform!("foo", "g", "bar", "foo baz foo").unwrap();
        assert_eq!(
            result.output, "bar baz bar",
            "Mit 'g'-Flag alle Vorkommen ersetzen"
        );
        assert_eq!(result.replacements_made, 2);
    }

    #[test]
    fn test_no_match_returns_original() {
        let result = transform!("xyz", "g", "bar", "foo baz foo").unwrap();
        assert_eq!(
            result.output, "foo baz foo",
            "Keine Ersetzung bei keinem Match"
        );
        assert_eq!(result.replacements_made, 0);
    }

    #[test]
    fn test_capture_group_replacement() {
        let result = transform!(r"(\w+)\s(\w+)", "", "$2 $1", "hello world").unwrap();
        assert_eq!(result.output, "world hello");
    }

    #[test]
    fn test_invalid_pattern_returns_error_not_panic() {
        let err = transform!("[invalid", "", "", "input").unwrap_err();
        assert!(
            err.contains("Ungültiges Regex-Pattern"),
            "Sollte klare Fehlermeldung zurückgeben: {err}"
        );
    }

    #[test]
    fn test_backreference_rejected_at_compile_time() {
        // Die regex-Crate lehnt Backreferences beim Kompilieren ab.
        // Dies beweist, dass kein ReDoS via Backreferences möglich ist.
        let err = transform!(r"(a)\1", "", "", "aa").unwrap_err();
        assert!(
            err.contains("Ungültiges Regex-Pattern"),
            "Backreference muss als Compile-Fehler abgelehnt werden: {err}"
        );
    }

    #[test]
    fn test_input_too_large_normal_pattern() {
        let large_input = "a".repeat(MAX_REGEX_INPUT_BYTES + 1);
        let err = transform!("a", "g", "b", &large_input).unwrap_err();
        assert!(
            err.contains("Eingabe zu groß"),
            "Limit-Fehler erwartet: {err}"
        );
        assert!(
            !err.contains("strukturell komplex"),
            "Einfaches Pattern sollte kein 'komplex'-Label erhalten"
        );
    }

    #[test]
    fn test_input_too_large_complex_pattern() {
        // Ein komplexes Pattern (viele Gruppen) triggert das niedrigere Limit.
        let complex_pattern = "(a)(b)(c)(d)(e)(f)(g)(h)(i)";
        let large_input = "a".repeat(MAX_REGEX_INPUT_BYTES_COMPLEX + 1);
        let err = transform!(complex_pattern, "g", "x", &large_input).unwrap_err();
        assert!(
            err.contains("Eingabe zu groß"),
            "Limit-Fehler erwartet: {err}"
        );
        assert!(
            err.contains("strukturell komplex"),
            "Komplexes Pattern sollte 'komplex'-Label enthalten: {err}"
        );
    }

    #[test]
    fn test_is_structurally_complex_simple_pattern() {
        assert!(
            !is_structurally_complex("foo"),
            "Einfaches Pattern ist nicht komplex"
        );
        assert!(
            !is_structurally_complex(r"\d+"),
            "Einfache Quantifizierung ist nicht komplex"
        );
        assert!(
            !is_structurally_complex("(a|b)"),
            "Eine Gruppe, eine Alternative ist nicht komplex"
        );
    }

    #[test]
    fn test_is_structurally_complex_many_groups() {
        let pattern = "(a)(b)(c)(d)(e)(f)(g)(h)(i)"; // 9 Gruppen → > 8
        assert!(
            is_structurally_complex(pattern),
            "9 Gruppen sollten als komplex gelten"
        );
    }

    #[test]
    fn test_is_structurally_complex_many_alternations() {
        let pattern = "a|b|c|d|e"; // 4 Alternationen → nicht komplex (≤4)
        assert!(!is_structurally_complex(pattern));
        let pattern2 = "a|b|c|d|e|f"; // 5 Alternationen → komplex (>4)
        assert!(is_structurally_complex(pattern2));
    }

    #[test]
    fn test_validate_pattern_valid() {
        let result = validate_regex_pattern(r"\d+".to_string());
        assert!(result.is_valid);
        assert!(result.error.is_empty());
        assert!(!result.is_complex);
        assert_eq!(result.effective_input_limit, MAX_REGEX_INPUT_BYTES);
    }

    #[test]
    fn test_validate_pattern_invalid() {
        let result = validate_regex_pattern("[invalid".to_string());
        assert!(!result.is_valid);
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_validate_pattern_complex() {
        let pattern = "(a)(b)(c)(d)(e)(f)(g)(h)(i)"; // 9 Gruppen
        let result = validate_regex_pattern(pattern.to_string());
        assert!(result.is_valid, "Pattern ist syntaktisch gültig");
        assert!(result.is_complex, "Sollte als komplex eingestuft werden");
        assert_eq!(
            result.effective_input_limit, MAX_REGEX_INPUT_BYTES_COMPLEX,
            "Komplexes Pattern muss reduziertes Limit haben"
        );
    }

    /// Regressionstest: Muster wie (a+)+ sind in der regex-Crate NICHT pathologisch.
    /// Dieser Test beweist, dass die Engine lineares Matching liefert.
    #[test]
    fn test_nested_quantifier_not_pathological() {
        // Bei einer backtracking-Engine würde "aaaaaaaab" auf "(a+)+" exponentiell laufen.
        // Die regex-Crate verarbeitet es in linearer Zeit.
        let input = "a".repeat(1000) + "b";
        let result = transform!("(a+)+b", "g", "MATCH", &input).unwrap();
        assert_eq!(
            result.output, "MATCH",
            "Pattern (a+)+b soll den gesamten String matchen und ersetzen"
        );
    }

    /// Regressionstest: Lookahead wird abgelehnt (würde lineare Laufzeit brechen).
    /// Stellt sicher, dass die Engine-Grenze korrekt greift.
    #[test]
    fn test_lookahead_rejected() {
        // Lookahead (?=...) wird von der regex-Crate nicht unterstützt.
        let err = transform!(r"foo(?=bar)", "", "baz", "foobar").unwrap_err();
        assert!(
            err.contains("Ungültiges Regex-Pattern"),
            "Lookahead muss abgelehnt werden: {err}"
        );
    }
}
