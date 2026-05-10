# Jules Scheduling-Kalender — MemFuse Development

> [!WARNING]  
> **DEPRECATED:** Scheduled tasks via Cron/Dashboard are obsolete. The system uses a dynamic, event-driven queue dispatch system (`jules-queue-dispatcher.yml`). Agents are invoked based on code `SUCCESSOR` anchors rather than rigid scheduling.
> **13 Accounts × 15 Scheduled Tasks/Tag**
> **Branch-Strategie:** Jeder Account → `jules/<account>/<wp-name>` → PR nach `dev`

---

## Tägliche Ausführungsreihenfolge

| UTC | Account | Rolle | Aktion |
|---|---|---|---|
| 05:00 | **13** | Debt Hunter | Tech-Debt Scans + Fixes (VOR allen Feature-Accounts) |
| 06:00 | **01** | Core Guardian | `memfuse-core` Stabilisierung |
| 07:00 | **02** | Store Engineer | `memfuse-store` WP-1.1/4.1 |
| 08:00 | **03** | Index Engineer | `memfuse-index` WP-2.2/4.3 |
| 09:00 | **04** | DB Orchestrator | `memfuse-db` WP-1.2/4.2 |
| 10:00 | **05** | Text Engine | `memfuse-text` WP-2.1 |
| 11:00 | **06** | Python Bindings | `memfuse-py` WP-3.1 |
| 12:00 | **10** | Security | Encryption WP-3.2 |
| 20:00 | **07** | QA Cross-Crate | Regressionen + Fixes |
| 21:00 | **12** | Integration Tester | E2E + Stress Tests |
| 22:00 | **09** | Benchmarks | Performance Tracking |

### Wöchentlich (Montag)
| UTC | Account | Rolle | Aktion |
|---|---|---|---|
| 08:00 | **08** | Docs & Specs | Dokumentation sync |
| 10:00 | **11** | CI/DevOps | Workflow-Optimierung |

---

## Phasenplan & Dependency-Gates

### Phase 0: Tech Debt (Woche 1)
```
[Account 13] ─── debt-audit + fixes ──→ gate: `just debt-audit` PASS
[Account 01] ─── WP-0.0 core cleanup ─→ gate: PR merged to dev
```
**Gate:** `just debt-audit` → 0 Violations. ALLE Accounts blocked bis Gate passiert.

### Phase 1: Core Stabilität (Woche 2-3)
```
[Account 02] ─── WP-1.1 Compaction ──→ gate: 5 ACs grün, Triple-Test
[Account 10] ─── WP-3.2 Encryption ──→ gate: 4 ACs grün (nach WP-1.1)
[Account 04] ─── WP-1.2 Collections ─→ gate: 4 ACs grün (nach WP-1.1)
```
**Parallele Arbeit möglich:** WP-3.2 und WP-1.2 beide abhängig von WP-1.1.

### Phase 2: Search Engines (Woche 3-4)
```
[Account 05] ─── WP-2.1 BM25/RRF ───→ gate: 4 ACs grün (nach WP-1.2)
[Account 03] ─── WP-2.2 SQ8 ────────→ gate: 4 ACs grün (nach WP-0.0)
[Account 04] ─── WP-4.2 Filtering ──→ gate: 2 ACs grün (nach WP-1.2)
```

### Phase 3: User Interface (Woche 4-5)
```
[Account 06] ─── WP-3.1 Python ─────→ gate: 3 Python Tests (nach WP-2.1)
[Account 02] ─── WP-4.1 mmap ───────→ gate: 2 ACs grün (nach WP-3.2)
```

### Phase 4: Scale (Woche 5+)
```
[Account 03] ─── WP-4.3 DiskANN ────→ gate: 2 ACs grün (nach WP-2.2+4.1)
```

---

## Wartende Accounts — Fallback-Aufgaben

Wenn ein Account blockiert ist (Dependency-Gate nicht passiert):

| Account | Wartende-Aufgaben |
|---|---|
| 02 | `.unwrap()` eliminieren, `std::fs` → `tokio::fs`, Test-Coverage Store |
| 03 | SAFETY-Kommentare in `distance.rs`, HNSW Edge-Case Tests |
| 04 | Bestehende Contract-Tests härten, Doc-Comments |
| 05 | Crate-Scaffold vorbereiten, Tokenizer-Prototyp in `/tmp/` |
| 06 | `Cargo.toml`/`pyproject.toml` Scaffold, pytest-Infrastruktur |
| 10 | Crypto-Crate-Evaluation, Dependency-Audit |

---

## Merge-Strategie

1. **Jeder Account** öffnet PRs gegen `dev`
2. **Account 07 (QA)** reviewed alle PRs (täglicher Cross-Check)
3. **Merge-Reihenfolge** folgt der Dependency-Chain (oben)
4. **Konflikte:** Account der den Konflikt verursacht, rebaset zuerst
5. **Release:** `dev` → `main` nur nach vollständigem `just triple-test`

---

## Dashboard-Setup Checkliste

Für jeden der 13 Accounts im Jules Dashboard konfigurieren:

- [ ] Account 01: Repository verbinden, Scheduled Task "Daily 06:00 UTC"
- [ ] Account 02: Repository verbinden, Scheduled Task "Daily 07:00 UTC"
- [ ] Account 03: Repository verbinden, Scheduled Task "Daily 08:00 UTC"
- [ ] Account 04: Repository verbinden, Scheduled Task "Daily 09:00 UTC"
- [ ] Account 05: Repository verbinden, Scheduled Task "Daily 10:00 UTC"
- [ ] Account 06: Repository verbinden, Scheduled Task "Daily 11:00 UTC"
- [ ] Account 07: Repository verbinden, Scheduled Task "Daily 20:00 UTC"
- [ ] Account 08: Repository verbinden, Scheduled Task "Weekly Mo 08:00 UTC"
- [ ] Account 09: Repository verbinden, Scheduled Task "Daily 22:00 UTC"
- [ ] Account 10: Repository verbinden, Scheduled Task "Daily 12:00 UTC"
- [ ] Account 11: Repository verbinden, Scheduled Task "Weekly Mo 10:00 UTC"
- [ ] Account 12: Repository verbinden, Scheduled Task "Daily 21:00 UTC"
- [ ] Account 13: Repository verbinden, Scheduled Task "Daily 05:00 UTC"
