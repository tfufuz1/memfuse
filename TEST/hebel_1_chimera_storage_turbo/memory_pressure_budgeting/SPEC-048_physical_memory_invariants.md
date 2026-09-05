# SPEC-048: Physical Memory Tier Invariants & MECE Track Definition

> **Stand:** 2026-04-18 | **Prio:** P0 | **Status:** 🟢 APPROVED
> **Mandat:** Google Alpha Production Stabilization
> **Ziel:** Eliminierung der Orchestration Tax durch native Engine-Integration.

---

## 1. MECE Implementierungs-Tracks

Um Überschneidungen zu vermeiden und parallele Entwicklung zu ermöglichen, wird die Sovereign Core Entwicklung in drei Mutually Exclusive and Collectively Exhaustive (MECE) Tracks unterteilt:

### Track A: Local Lifecycle & Memory Mapping
- **Fokus:** Latenz-Eliminierung innerhalb eines einzelnen Nodes.
- **Kernkomponenten:** `chimera-ql`, `chimera-agent`, `chimera-core::TxBuffer`.
- **Ziel:** 0ms "Decision-to-Storage" Overhead für Working Memory.

### Track B: Distributed Mesh & Compute Isolation
- **Fokus:** Durchsatz-Skalierung und Cluster-Resilienz.
- **Kernkomponenten:** `chimera-net` (QUIC), `chimera-wasm` (In-DB Filter), `chimera-distributed`.
- **Ziel:** Parallelisierung von Semantic Recall ohne Beeinträchtigung der Ingest-Pipeline.

### Track C: I/O Path & Storage Evolution
- **Fokus:** Maximierung der Hardware-Auslastung.
- **Kernkomponenten:** `chimera-uring`, `chimera-colfam` (Columnar Storage).
- **Ziel:** Sättigung von NVMe-Bandbreiten durch Zero-Copy Pfade.

---

## 2. Physikalische Invarianten des Agentic Memory

Das Agentic Memory Modell wird von einer logischen Abstraktion in strikte physische Speichergarantien transformiert.

| Tier | Physisches Substrat | Invariante | Performance-Ziel |
|:-----|:--------------------|:-----------|:-----------------|
| **Working (WM)** | Lock-Free `TxBuffer` (RAM) | **INV-M1:** Daten verlassen niemals den RAM vor dem Commit. Keine WAL-Interaktion für WM-Queries. | < 100μs Latenz |
| **Episodic (EM)** | Ring-Buffered Segmented WAL | **INV-M2:** EM-Einträge sind über einen Append-Only Time-Index adressierbar. Automatisches TTL-Pruning nach Segment-Roll. | O(1) Sequential Read |
| **Semantic (SM)** | HNSW + Column-Family LSM | **INV-M3:** Persistenz via 2PC. Zugriff nur über optimierte Vektor-Indizes. Hintergrund-Kompaktierung zur Rauschunterdrückung. | O(log N) Search |

---

## 3. Strategische Invarianten für die Produktion

### INV-P1: Zero Orchestration Tax
Jeder Memory-Zugriff eines Agenten muss direkt auf die physische Schicht mappen. Es darf keine "Orchestration Layer" (wie LangChain-Provider) zwischen der Agent-Logik und dem Chimera-Speicher geben.

### INV-P2: Track-Isolation
Änderungen in Track C (Storage) dürfen die API-Stabilität von Track A nicht gefährden. Der `StorageEngine`-Trait fungiert als unüberwindbare Barriere.

### INV-P3: Resource Budgeting (SPEC-032)
Working Memory Allokationen unterliegen einem harten Limit pro Namespace, um OOM-Kaskaden bei außer Kontrolle geratenen Agenten-Loops zu verhindern.
