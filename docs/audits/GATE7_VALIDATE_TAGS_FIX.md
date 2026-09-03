# Audit Report: Gate 7 Dynamischer ISO-8601 Tag-Validator (`cargo xtask validate-tags`)

> **Datum:** September 2026
> **Aufgabe:** Prompt P1-A — Gate 7: Dynamischer ISO-8601 Tag-Validator
> **Status:** Passed / Behalten

---

## 1. Alter vs. Neuer Gate-7-Code

### Alter Gate-7-Code (`.github/workflows/context-gates.yml`)

```yaml
      - name: "Gate 7: TS UND SESSION Pflichtfelder auf allen neuen Tags"
        run: |
          # Prüfe TS-Feld auf allen Tags
          MISSING_TS=$(grep -rEn "AI-TAG\[|ANCHOR\[" crates/ --include="*.rs" \
            | grep -vE "TS:[0-9]{4}-[0-9]{2}-[0-9]{2}T" || true)
          if [ -n "$MISSING_TS" ]; then
            echo "❌ Tags ohne gültigen TS:-Zeitstempel:"
            echo "$MISSING_TS"
            exit 1
          fi

          # Prüfe SESSION-Feld auf NEUEN Tags (Datum >= 2026-08-29)
          MISSING_SESSION=$(grep -rEn "AI-TAG\[|ANCHOR\[" crates/ --include="*.rs" \
            | grep -E "TS:2026-08-(29|30|31)T|TS:2026-(09|1[0-2])-|TS:(202[7-9]|20[3-9][0-9]|2[1-9][0-9]{2}|[3-9][0-9]{3})-" \
            | grep -v "SESSION:" || true)
          if [ -n "$MISSING_SESSION" ]; then
            echo "❌ Neue Tags (>= 2026-08-29) ohne SESSION:-Feld:"
            echo "$MISSING_SESSION"
            echo "Füge SESSION: <8-hex> zu diesen Tags hinzu."
            exit 1
          fi
          echo "✅ Alle Tags haben TS: und neue Tags haben SESSION: Felder"
```

### Neuer Gate-7-Code (`.github/workflows/context-gates.yml`)

```yaml
      - name: "Gate 7: TS & SESSION Pflichtfelder (dynamisch via xtask)"
        run: |
          cargo run -p xtask -- validate-tags || {
            echo "💡 CI-FIXER GUIDANCE: Füge fehlende (TS:...) (SESSION:...) Felder zu den oben"
            echo "   genannten Tags hinzu. Format: TS:YYYY-MM-DDTHH:MM:SSZ SESSION:8hexchars"
            exit 1
          }
          echo "✅ Alle Tags haben gültige TS: und SESSION: Felder"
```

---

## 2. Neue Unit-Tests in `xtask/src/main.rs`

Die folgenden 5 Unit-Tests wurden hinzugefügt:

1. `test_validate_tags_no_ts_field`: Prüft, dass ein Tag ohne `TS:`-Feld als ungültig erkannt wird (`false`).
2. `test_validate_tags_invalid_timestamp`: Prüft, dass ein ungültiger ISO-8601 Timestamp (z. B. `2026-13-01T00:00:00Z`) abgelehnt wird (`false`).
3. `test_validate_tags_before_cutoff_without_session`: Prüft, dass ein Tag vor dem Cutoff (`2026-08-28T12:00:00Z`) auch ohne `SESSION:` zulässig ist (`true`).
4. `test_validate_tags_after_cutoff_without_session`: Prüft, dass ein Tag ab dem Cutoff (`2026-08-29T00:00:00Z`) ohne `SESSION:` fehlschlägt (`false`).
5. `test_validate_tags_after_cutoff_with_session`: Prüft, dass ein Tag ab dem Cutoff mit `SESSION:` akzeptiert wird (`true`).

---

## 3. Verifikationslogs

### `cargo test -p xtask`
```text
running 17 tests
test tests::test_check_consistency_fails_on_duplicate_adr_number ... ok
test tests::test_check_consistency_fails_on_crate_missing_agents_md ... ok
test tests::test_check_consistency_passes_on_current_decisions ... ok
test tests::test_check_review_coverage_fixtures ... ok
test tests::test_context_tags_filtering_by_severity ... ok
test tests::test_context_tags_filtering_by_crate ... ok
test tests::test_context_tags_filtering_by_status ... ok
test tests::test_hash_id_collision_freedom ... ok
test tests::test_validate_tags_after_cutoff_with_session ... ok
test tests::test_validate_tags_after_cutoff_without_session ... ok
test tests::test_validate_tags_before_cutoff_without_session ... ok
test tests::test_validate_tags_invalid_timestamp ... ok
test tests::test_validate_tags_no_ts_field ... ok
test tests::test_check_consistency_passes_on_clean_fixture ... ok
test tests::test_run_check_review_coverage_compiles_and_runs_without_panic ... ok
test tests::test_working_state_single_marker_block ... ok
test tests::test_run_check_consistency_compiles_and_runs_without_panic ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

### `cargo run -p xtask -- validate-tags`
```text
=== Running xtask validate-tags ===
✅ Alle Tags haben gültige TS: und SESSION: Felder
```

### `cargo check --workspace --exclude memfuse-tauri`
```text
    Checking xtask v0.1.0 (/app/xtask)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.84s
```

### `cargo clippy -p xtask -- -D warnings`
```text
    Checking xtask v0.1.0 (/app/xtask)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.63s
```
