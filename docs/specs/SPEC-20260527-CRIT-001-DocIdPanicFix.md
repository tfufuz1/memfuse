# Atomic Spec: CRIT-001 DocId Panic Resolution

## 1. Problemstellung
Die Methode `DocId::from_key()` in `memfuse-core` nutzt (laut Audit-Report) `.expect()`, was gegen die **Sovereign Core Doctrine** (Zero-Panic) verstößt. Obwohl die aktuelle Implementierung bereits `Result` zurückgibt, muss sichergestellt werden, dass alle Panics entfernt sind, die Signatur stabil auf `Result<DocId, MemFuseError>` bleibt und alle Callers im Workspace den Fehler korrekt propagieren.

## 2. Zielzustand
- `DocId::from_key` und `DocId::try_from_key` geben `Result<Self, MemFuseError>` zurück.
- Keine Verwendung von `.unwrap()`, `.expect()` oder `panic!()` in diesen Methoden.
- Alle Callers in `memfuse-db`, `memfuse-index`, etc. nutzen den `?`-Operator.
- Der Audit-Status für CRIT-001 in `AGENTS.md` wird auf ✅ (Resolved) gesetzt.

## 3. Implementierungsplan (TDD)
1. **RED**: Testfall in `memfuse-core` hinzufügen, der die Infallibilität (oder korrekte Fehlerbehandlung) von `from_key` verifiziert.
2. **GREEN**: Sicherstellen, dass die Implementierung in `domain.rs` absolut sauber ist.
3. **REFACTOR**: Workspace-weiten Scan nach `from_key().unwrap()` oder `.expect()` durchführen und fixen.
4. **VALIDATE**: `just test` und `just debt-audit`.

## 4. Invarianten
- [INV-CORE-1] Keine Panics in Layer 0 Typ-Konvertierungen.
- [INV-CORE-2] Blake3 Hash-Länge wird sicher behandelt.
