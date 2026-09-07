## Pull Request Checkliste

### Code-Dimension (CI, Pflicht)
- [ ] `cargo test --workspace` grün
- [ ] `just dag-check` / `cargo xtask check-dag` grün (P1)
- [ ] `cargo clippy --workspace -- -D warnings` grün
- [ ] Kein neues `unsafe` ohne `// SAFETY:` (P2)
- [ ] Kein neues `let _ =` auf I/O (P3)

### Architektur-Dimension
- [ ] **Reuse-Check (P10):** Wurde geprüft ob bestehende Funktionen (`score_batch()`, `tombstoned_edges`, `persist_calibration_state`) wiederverwendet werden können?
  - Befund: _______________
- [ ] **ADR eingetragen (P6):** Falls architektonisch relevant — ADR-Nummer: ___

### Kalibrierungs-Dimension (P8, wenn Konfiguration geändert)
- [ ] Änderung an SlmProfile/Prompt-Template/Modell-Config → Kalibrierungs-Reset-Test vorhanden
- [ ] Keine neue Ad-hoc-Sigmoid-Logik — stattdessen `memfuse-calibration` verwendet

### Provenienz-Dimension (wenn Graph-Code geändert)
- [ ] Neue CSR-Kante hat `EdgeProvenance`-Eintrag (INV-GRAPH-PROV-1)
- [ ] RRF-Fusion mit Kohärenz-Bonus: INV-PROV-2 eingehalten

### Quantitative Aussagen (P7)
- [ ] Keine neuen quantitativen Claims ohne Nachweis in `memfuse-bench` oder ArXiv-Kennzeichnung

### Chaos-Dimension (nur bei Storage-Änderungen)
- [ ] Power-Cut-Simulation für WAL/Compaction
- [ ] Zeroize-Nachweis wenn KV-Cache-Code berührt (P9)

### Nicht-Implementieren-Prüfung
- [ ] Kein partieller HNSW-Rebuild (VETO-01 / ADR-071)
- [ ] Keine Cross-Tenant-Datenbewegung (VETO-02 / ADR-082)

---

## Beschreibung der Änderung

### Was wurde geändert?


### Warum?


### Welche Invarianten werden berührt?
(aus AGENTS.md Invarianten-Katalog)


### Bekannte Einschränkungen / offene Folgepunkte
