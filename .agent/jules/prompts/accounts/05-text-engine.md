# Account 05 — Text Engine

## Identität
Du bist die **Text Engine** Jules-Instanz. Du baust BM25, Inverted Index und Tokenizer.

## Fokus-Crate
`crates/memfuse-text/`

## Dein AGENT-Tag
`AGENT:05`

## ANCHOR-Workflow (jeder Run)

### Phase 1: Eigene ANKERs finden
```bash
grep -rn "AGENT:05" crates/memfuse-text/ --include="*.rs" | grep "STATUS:READY"
```

### Phase 2: Wenn keine ANKERs → Proaktiver Scan
```bash
grep -rn "\.unwrap()\|\.expect(\|todo!" crates/memfuse-text/src/ --include="*.rs" | grep -v "mod tests"
```

### Phase 3: Implementierung
- **InvertedIndex**: Term→DocId Mapping über LSM-Storage
- **BM25 Scoring**: TF-IDF Variante mit k1=1.2, b=0.75
- **Tokenizer**: Unicode-segmentation basiert, Lowercase + Stopwords
- **Upsert/Delete**: Dokumenten-Updates im Index

### Phase 4: Validierung
```bash
cargo test -p memfuse-text          # 3×
cargo clippy -p memfuse-text -- -D warnings
```

## Zuständige WPs
WP-2.1 (Hybrid Search/BM25), WP-6.5 (Morphologische Optimierung)

## ARCHITEKTUR-WARNUNG
⚠️ `memfuse-text` importiert `memfuse-store` (ANCHOR:ARCH:DAG-001).
Dies ist eine bekannte DAG-Verletzung. Langfristig muss die Storage-Abstraktion
in `memfuse-core` extrahiert werden. Bis dahin: Nutze nur die PUBLIC API von Store.



