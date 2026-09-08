# MemFuse — Jules Agent Context
> Version: 2.1 | Stand: 2026-09-08 | Permanent Ambient Context für Jules Sessions
>
> ⚠️ **FRISCHEGARANTIE**: Diese Datei regelt ausschließlich die Session-Prozessführung für Jules.
> Die tatsächlichen Code-Fakten, Crate-Strukturen, Invarianten und Implementierungsstände
> sind gemäß MECE-Prinzip (CONSTITUTION.md §Documentation Model) in folgenden Quellen verankert:
> - **Code-Zustand & Non-Obvious Decisions**: siehe `AGENTS.md`
> - **Dynamischer Projektstatus & Tag-Inventar**: siehe `WORKING_STATE.md`
> - **Architektur-Entscheidungen (ADRs)**: siehe `DECISIONS.md` / `docs/decisions/`

---

## 🎯 Kontext-Ladeordnung & Modus Operandi für Sessions

Um Halluzinationen und veraltete Fakten zu vermeiden, gilt für jede Jules-Session folgende Lade- und Nachschlage-Reihenfolge:
1. **System & Arbeitsumgebung**: `.jules/JULES_CONTEXT.md` (Prozessanleitung), `.jules/SESSION_BOOTSTRAP.md`
2. **Aktueller Code-Zustand & Invarianten**: `AGENTS.md` (Verifizierter Code-Befund, Crate-Topologie, Non-Obvious Decisions)
3. **Offene Schulden & Tags**: `WORKING_STATE.md` (Autogenerierter Tag-Bericht)
4. **Verbindliche Architektur-Vorgaben**: `DECISIONS.md` (ADR-Zusammenfassungen)

---

## 📐 Crate-Topologie & Referenzen

Vollständige Schichten-Architektur (Layer 0–6) sowie DAG-Regeln: siehe `AGENTS.md` Abschnitt **"Crate-Topologie"**.

---

## 📖 Crate-AGENTS.md Laderegel

**MANDATORY FIRST STEP:** Bevor Code in einer Crate bearbeitet wird, MUSS Jules die jeweilige `AGENTS.md` der Crate laden. Sie enthält Modul-Karten, API-Signaturen, Anti-Patterns und Lock-Hierarchien.

| Crate | Pfad für view_file / read |
|---|---|
| `memfuse-core` | `crates/memfuse-core/AGENTS.md` |
| `memfuse-store` | `crates/memfuse-store/AGENTS.md` |
| `memfuse-index` | `crates/memfuse-index/AGENTS.md` |
| `memfuse-text` | `crates/memfuse-text/AGENTS.md` |
| `memfuse-crypto` | `crates/memfuse-crypto/AGENTS.md` |
| `memfuse-graph` | `crates/memfuse-graph/AGENTS.md` |
| `memfuse-checkpoint` | `crates/memfuse-checkpoint/AGENTS.md` |
| `memfuse-db` | `crates/memfuse-db/AGENTS.md` |
| `memfuse-agent` | `crates/memfuse-agent/AGENTS.md` |
| `memfuse-ollama` | `crates/memfuse-ollama/AGENTS.md` |
| `memfuse-embed` | `crates/memfuse-embed/AGENTS.md` |
| `memfuse-py` | `crates/memfuse-py/AGENTS.md` |
| `memfuse-router` | `crates/memfuse-router/AGENTS.md` |
| `memfuse-mcp` | `crates/memfuse-mcp/AGENTS.md` |
| `memfuse-tauri` | `crates/memfuse-tauri/AGENTS.md` |

---

## 🚫 Architektur-Entscheidungen (ADRs)

Vollständige Liste und Verbindlichkeit aller Architektur-Entscheidungen: siehe `DECISIONS.md` sowie `AGENTS.md` Abschnitt **"Non-Obvious Decisions"**.

---

## ✅ Existierende Typen & API-Disziplin

- **Existierende Typen**: Die Übersicht aller bereits verifizierten Typen und Module befindet sich in `AGENTS.md` Abschnitt **"Was TATSÄCHLICH implementiert ist"**. Vor jeder Neuimplementierung zusätzlich `find crates/ -name "*.rs" | xargs grep -l "<Typ-Name>"` ausführen.
- **Kritische Implementierungs-Muster**: Details zu `TxId`-Allokation, `fsync`-Fehlerbehandlung, `unsafe`-Einschränkungen und HMAC-Keys sind zentral in `AGENTS.md` unter **"Non-Obvious Decisions"** hinterlegt.
