# JULES.md — Google Jules Agent Adapter
> Importiert alle Regeln aus @AGENTS.md. Nur Jules-spezifische Ergänzungen.

@AGENTS.md
@WORKING_STATE.md

## Jules-Spezifika

### Kontext-Ladestrategie (immer, zu Beginn)

1. `AGENTS.md` — ambient (bereits geladen via @-Import)
2. `WORKING_STATE.md` — ambient (auto-generiert, immer frisch)
3. `.jules/SESSION_BOOTSTRAP.md` — Session-Checkliste ausführen
4. `crates/<CRATE>/AGENTS.md` — wenn du Code in einem Crate schreibst
5. Relevante `rules/*.md` — basierend auf Aufgabentyp (siehe SESSION_BOOTSTRAP §Phase 3)

### Nix/Cargo Fallback

Jules-Environment hat nicht immer eine Nix-Shell. Fallback-Reihenfolge:
```bash
# Primär (mit Nix):
nix develop -c cargo check --workspace --exclude memfuse-tauri

# Fallback (ohne Nix, direkt):
cargo check --workspace --exclude memfuse-tauri
```

Die `justfile`-Befehle haben alle `|| cargo ...` Fallbacks.

### Kontext-Übersicht für schnelle Navigation

| Ich brauche... | Datei |
|----------------|-------|
| Aktuelle offene Probleme | `WORKING_STATE.md` (autogeneriert) |
| Operative Regeln | `AGENTS.md` |
| Projektprinzipien (Warum) | `CONSTITUTION.md` (on-demand) |
| Architektur-Entscheidungen | `DECISIONS.md` (on-demand) |
| Domain-Begriffe | `GLOSSARY.md` |
| Bekannte LLM-Fehler | `.jules/COMMON_LLM_ERRORS.md` |
| Session-Start | `.jules/SESSION_BOOTSTRAP.md` |
| Audit-Verifikation | `.jules/AUDIT_INTAKE_PROTOCOL.md` |
| Typ-Kollisionsschutz | `docs/TYPE_REGISTRY.md` |
| SIMD/unsafe Regeln | `rules/simd_safety.md` |
| WAL/Crypto Regeln | `rules/wal_crypto.md` |
| Error Handling | `rules/error-handling.md` |
| Testing | `rules/testing.md` |
| Async I/O | `rules/async-io.md` |
| Dependencies | `rules/dependencies.md` |
