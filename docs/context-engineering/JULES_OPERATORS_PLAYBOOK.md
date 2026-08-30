# Jules Operator's Playbook
**Praktische Workflows für Google-Jules mit MemFuse Context System**

Version: 2.0 · Zielgruppe: Human Reviewers + Jules Agent Orchestrator
Status: PRODUCTION · Datum: 2026-08-29

---

## INTRO: ROLLEN & VERANTWORTUNG

### Rollen im LLM-Entwicklungsworkflow

| Rolle | Verantwortung | Tools |
|-------|---------------|-------|
| **Google-Jules (LLM-Agent)** | Code-Generierung, Plan-Erstellung, Selbst-Review | `context-cli`, `cargo xtask audit-*` |
| **Human Reviewer** | Security-Gate-Checks, Unsafe-Code-Validierung, ADR-Approval | GitHub PR interface, `cargo test`, compliance tools |
| **Security Lead** | Audit-Finding-Triaging, CRITICAL-Issue-Approval | `cargo xtask audit-verify`, `REVIEW-PASS` logging |
| **Architecture Lead** | API-Signature-Review, DAG-Consistency, ADR-Writing | `just dag-check`, CONSTITUTION.md, DECISIONS.md |

---

## WORKFLOW 1: TASK INITIALIZATION (Jules Start)

### Scenario: Jules starts new session for feature development

```bash
# ═══════════════════════════════════════════════════════════════════
# 1. VM SETUP (Automated by deployment)
# ═══════════════════════════════════════════════════════════════════
#!/usr/bin/env bash
cd /app
./env-setup.sh  # Generates SESSION_ID, loads diagnostics

# Output:
# ============================================================
#   MemFuse Jules Environment Ready
#   Session ID: a3f29c1d
#   Open Blockers: 1
#   Open Anchors: 2
# ============================================================
```

### Phase 1: Load Global Context (2 min)

```bash
# ═══════════════════════════════════════════════════════════════════
# Automatische Context-Laden (via .bashrc alias)
# ═══════════════════════════════════════════════════════════════════
source ~/.bashrc
jules-start

# Output:
# 🚀 Starting MemFuse Jules Session...
# 📖 Loading AGENTS.md + WORKING_STATE.md...
#
# # AGENTS.md — MemFuse Operative Agent Rules
# > Version 6.0 · Ambient context — always loaded
# ...
#
# 🚨 CRITICAL BLOCKERS (must fix):
# [
#   {
#     "id": "AGT-DB-a3f29c1d",
#     "category": "CONCURRENCY",
#     "severity": "CRITICAL",
#     "status": "OPEN",
#     "file": "crates/memfuse-db/src/collection/relate.rs:123"
#   }
# ]
#
# ✅ Session ID: a3f29c1d
# ✅ Ready to code. Run 'context-cli help' for CLI info.
```

### Phase 2: Validate No Regressions (1 min)

```bash
# Schnelle Checks
just check          # cargo fmt + clippy + cargo check
cargo test --lib   # Unit tests (fast)

# Bei Fehlern:
cargo xtask context-digest --format text | head -20
# → Zeigt neueste Probleme für schnelle Diagnose
```

### Phase 3: Load Crate-Specific Context (3 min)

Angenommen: Jules muss memfuse-db modifizieren

```bash
# Lade Crate AGENTS.md + Struktur
cargo xtask context-crate memfuse-db --format text | less

# Output (strukturiert):
# ═══════════════════════════════════════════════════════════════════
# CRATE: memfuse-db
# LAYER: L3 (Transaction Coordinator)
# ═══════════════════════════════════════════════════════════════════
#
# AGENTS.md EXCERPT:
# > Non-obvious decisions...
# - TxId generation: ALWAYS collection.allocate_tx()
# - Snapshot rollback: Requires RelateGuard (ADR-023)
# ...
#
# OPEN BLOCKERS:
# AGT-DB-a3f29c1d: CONCURRENCY[CRITICAL] relate() race condition
#
# OPEN ANCHORS:
# ANCHOR[INTEGRATION:WP-7.1]: Wire MarkdownChunker (STATUS: OPEN)
#
# KEY FILES:
# - src/collection/relate.rs (open issues: 1)
# - src/collection/mod.rs (role: TransactionManager)
#
# DEPENDENCIES: memfuse-core (L1), memfuse-crypto (sibling)
```

---

## WORKFLOW 2: ISSUE FIXING (Jules Execution)

### Scenario: Implement fix for CRITICAL AI-TAG

**Angenommen: AGT-DB-a3f29c1d ist offen → Jules muss fixen**

```bash
# ═══════════════════════════════════════════════════════════════════
# SCHRITT 1: Verstehe das Problem
# ═══════════════════════════════════════════════════════════════════

# Zeige spezifische Issue
cargo xtask context-tags --severity CRITICAL --status OPEN --crate memfuse-db

# Output (NDJSON):
# {"id":"AGT-DB-a3f29c1d","severity":"CRITICAL","file":"...relate.rs:123",...}

# Zeige Datei-Kontext
cargo xtask context-file crates/memfuse-db/src/collection/relate.rs

# Output:
# === FILE CONTEXT HEADER ===
# STAND:    2026-08-29T09:14:07Z
# ZWECK:    Transaction relation and rollback semantics
# SCOPE:    Crate: memfuse-db | Layer: L3 | Role: RelateCoordinator
# INVARIANTEN: sequence_id increases monotonically, fsync() errors propagate
#
# === OPEN ISSUES (THIS FILE) ===
# AI-TAG[CONCURRENCY][CRITICAL] Race condition in snapshot rollback (ID: AGT-DB-a3f29c1d)
#   STATUS: OPEN
#   BEFUND: relate() liest collection state ohne Locking, concurrent flush() schreibt WAL
#   RISIKO: Stale snapshot reads, potential data loss on rollback
#   EMPFEHLUNG: Acquire RelateGuard before state inspection (ADR-023 model)
```

### Schritt 2: Überprüfe ADR + Related Code

```bash
# Lese ADR-023 für Pattern-Vorlage
grep -A 20 "ADR-023" DECISIONS.md

# Output:
# ## ADR-023: Synchronization Guard Patterns
#
# Context: Concurrent access to shared state requires safe synchronization.
#
# Decision: Use guard wrappers (e.g., RelateGuard) to ensure atomic access.
#   Example:
#   ```rust
#   let guard = collection.acquire_relate_guard()?;
#   let state = guard.read_state();  // Safe
#   ```
# ...

# Suche existierende Guard-Implementierungen als Vorlage
grep -r "Guard" crates/memfuse-db/src --include="*.rs" | grep "struct\|impl"
# → Finde RelateGuard oder ähnliche Patterns
```

### Schritt 3: Implementiere Fix

```rust
// crates/memfuse-db/src/collection/relate.rs:123

// VORHER (BUGGY):
pub fn relate(&mut self, tx_id: TxId, ...) -> Result<()> {
    // ❌ KEINE SYNCHRONISIERUNG!
    let state = self.read_state();  // Race condition!
    // ... process ...
}

// NACHHER (FIXED):
pub fn relate(&mut self, tx_id: TxId, ...) -> Result<()> {
    // ✅ Acquire guard BEFORE state access
    let guard = self.acquire_relate_guard()?;
    let state = guard.read_state();  // Now safe (ADR-023)
    // ... process ...
}

// RESOLVED: AGT-DB-a3f29c1d — relate() now uses RelateGuard (TS: 2026-08-29T10:15:00Z)
```

### Schritt 4: Teste Fix

```bash
# ═══════════════════════════════════════════════════════════════════
# GATE: cargo test -p memfuse-db (von AGENTS.md)
# ═══════════════════════════════════════════════════════════════════

cargo test -p memfuse-db --test concurrent_collection_stress

# Output:
# test concurrent_collection_stress ... ok
# test relate_guards_prevent_race ... ok

cargo test --workspace --exclude memfuse-tauri

# All tests green ✅
```

### Schritt 5: Formatiere + Sync Docs

```bash
cargo fmt --all

# Sync automatically removes RESOLVED tags and updates WORKING_STATE.md
just sync-docs

# Output:
# [INFO] Syncing documentation from inline tags...
# [INFO] Removed RESOLVED tags: 1
# [INFO] Updated WORKING_STATE.md
# [INFO] Generated CHANGELOG entry for AGT-DB-a3f29c1d
# [SUCCESS] Documentation synced
```

### Schritt 6: Erstelle Pull Request

```bash
git add -A
git commit -m "Security: Fix race condition in relate() (AGT-DB-a3f29c1d)

- Implemented RelateGuard wrapper for synchronization (ADR-023)
- All concurrency tests pass
- Audit finding AUDIT-2026-09-001 addressed

Required: 3 REVIEW-PASS for security changes"

git push origin feature/fix-relate-race-condition
# → GitHub creates PR, requires reviews
```

---

## WORKFLOW 3: AUDIT FINDING INTAKE (External Audit)

### Scenario: Security audit discovers vulnerability

**Input:** External firm liefert Audit-Report mit Findings

```json
{
  "finding_id": "AUDIT-2026-09-002",
  "severity": "HIGH",
  "category": "MEMORY-SAFETY",
  "title": "Integer overflow in vector allocation",
  "file": "crates/memfuse-index/src/hnsw.rs",
  "line": 456,
  "description": "Vector capacity calculation does not check for overflow"
}
```

### Schritt 1: Validate Finding (Jules)

```bash
# ═══════════════════════════════════════════════════════════════════
# Automatische Validierung: Ist das Problem noch aktuell?
# ═══════════════════════════════════════════════════════════════════

cargo xtask audit-verify AUDIT-2026-09-002 \
  --file crates/memfuse-index/src/hnsw.rs \
  --line 456

# Output:
# ═══════════════════════════════════════════════════════════════════
# AUDIT-2026-09-002: VALID ✓
# ├─ File exists: crates/memfuse-index/src/hnsw.rs
# ├─ Line 456 still contains vulnerable code:
# │  454│     let capacity = count * size;  // ❌ NO OVERFLOW CHECK
# │  455│     allocate_vector(capacity)
# │
# ├─ No existing AI-TAG at this location
# └─ Recommendation: Create AI-TAG[MEMORY-SAFETY][CRITICAL]
# ═══════════════════════════════════════════════════════════════════
```

### Schritt 2: Create Tracking AI-TAG (Jules)

```rust
// crates/memfuse-index/src/hnsw.rs:454

// AI-TAG[MEMORY-SAFETY][CRITICAL] Integer overflow in vector allocation (Audit Finding)
// ID:       AGT-INDEX-b4e7f2c5
// TS:       2026-08-29T11:30:00Z
// SESSION:  a3f29c1d
// STATUS:   OPEN
// AUDIT_ID: AUDIT-2026-09-002
// SOURCE:   External Security Audit (Firm X)
// BEFUND:   Vector capacity calculation (count * size) does not check for overflow
// RISIKO:   Allocation can wrap around, causing buffer overflow on large collections
// EMPFEHLUNG: Use checked_mul() or use saturating_mul() + bounds check
```

### Schritt 3: Implement Fix (Jules)

```rust
// crates/memfuse-index/src/hnsw.rs:454 (FIXED)

let capacity = count
    .checked_mul(size)
    .ok_or(MemFuseError::AllocationOverflow)?;  // ✅ Safe
allocate_vector(capacity)

// RESOLVED: AUDIT-2026-09-002 — Integer overflow prevented with checked_mul (TS: 2026-08-29T12:00:00Z)
```

### Schritt 4: Create REVIEW-PASS Chain

**Human Reviewer 1** (Internal Security):
```rust
// REVIEW-PASS[1/3] Audit fix validation
// ID:       AGT-INDEX-b4e7f2c5
// TS:       2026-08-29T12:15:00Z
// SESSION:  b8e4f1a2
// STATUS:   PASS
// KONTEXT:  FRESH
// BEFUND:   checked_mul() correctly handles overflow; fallible error propagation validated
```

**Human Reviewer 2** (Code Owner):
```rust
// REVIEW-PASS[2/3] Implementation review
// ID:       AGT-INDEX-b4e7f2c5
// TS:       2026-08-29T12:30:00Z
// SESSION:  c9f5g3b3
// STATUS:   PASS
// KONTEXT:  CARRIED_FORWARD
// BEFUND:   Fix aligns with error-handling patterns in ADR-009; all related tests pass
```

**Human Reviewer 3** (External Auditor — Optional):
```rust
// REVIEW-PASS[3/3] External auditor confirmation
// ID:       AGT-INDEX-b4e7f2c5
// TS:       2026-08-29T13:00:00Z
// SESSION:  d0g6h4c4
// STATUS:   PASS
// KONTEXT:  CARRIED_FORWARD
// BEFUND:   Confirms fix adequately addresses AUDIT-2026-09-002 severity and scope
```

### Schritt 5: Merge to Main

```bash
# Alle 3 REVIEW-PASSes vorhanden → Merge approval

git merge --squash feature/audit-2026-09-002

# Commit message includes audit traceability:
# "Security: Fix integer overflow in vector allocation (AUDIT-2026-09-002)
#
# - Used checked_mul() to prevent capacity wraparound (ADR-009)
# - 3 REVIEW-PASS from internal + external auditors
# - All safety tests pass"
```

---

## WORKFLOW 4: MULTI-AGENT ORCHESTRATION

### Scenario: Multiple Jules instances working in parallel (enterprise-scale)

```
┌─────────────────────────────────────────────────────────────┐
│ GLOBAL SESSION COORDINATOR                                  │
│ ├─ SESSION:a3f29c1d (Jules-1, Feature A)                   │
│ ├─ SESSION:b8e4f1a2 (Jules-2, Feature B)                   │
│ └─ SESSION:c9f5g3b3 (Jules-3, Security Audit Fixes)        │
└─────────────────────────────────────────────────────────────┘
```

#### Problem: Multiple Sessions accessing same files

```bash
# ═══════════════════════════════════════════════════════════════════
# WORKFLOW: Merge conflict prevention
# ═══════════════════════════════════════════════════════════════════

# Bevor Jules-2 auf memfuse-db arbeitet:
cargo xtask context-crate memfuse-db --format json \
  | jq '.open_anchors[] | select(.status == "IN-PROGRESS")'

# Output:
# {
#   "id": "ANCHOR[INTEGRATION:WP-7.1]",
#   "agent": "AGENT:12",
#   "status": "IN-PROGRESS",
#   "session": "a3f29c1d",
#   "gate": "cargo test -p memfuse-db --test integration_chunker"
# }

# Decision: Jules-2 wartet, bis Jules-1 DONE ist (ADR-XXX Koordination)
# oder arbeitet parallel in nicht-overlappenden Dateien:
# - Jules-1: src/collection/relate.rs
# - Jules-2: src/checkpoint/mod.rs
```

#### Audit Trail für Multi-Agent Koordination

```bash
# WORKING_STATE.md wird auto-updated:

| AGENT | Session | Task | File(s) | Status | ETA |
|-------|---------|------|---------|--------|-----|
| AGENT:12 | a3f29c1d | Security: relate() fix | relate.rs | IN-PROGRESS | 2026-08-29 |
| AGENT:13 | b8e4f1a2 | Feature: Chunker integration | checkpoint.rs | OPEN | 2026-08-30 |
| AGENT:14 | c9f5g3b3 | Audit fixes (AUDIT-2026-09-*) | hnsw.rs | IN-PROGRESS | 2026-08-29 |
```

---

## WORKFLOW 5: HUMAN REVIEW PROCESS

### Scenario: PR comes in with Jules-generated code

**Jules created PR #1234:**
```
Title: Security: Fix race condition in relate() (AGT-DB-a3f29c1d)
Description:
- Implemented RelateGuard wrapper for synchronization (ADR-023)
- All concurrency tests pass
- Audit finding AUDIT-2026-09-001 addressed
- Required: 3 REVIEW-PASS for security changes
```

### Checklist für Reviewer (Human)

```markdown
## Security Review (5 min)

- [ ] Read AI-TAG context: grep "AGT-DB-a3f29c1d" crates/**/*.rs
- [ ] Verify ADR-023 compliance: cat DECISIONS.md | grep -A 10 ADR-023
- [ ] Check guard acquisition pattern:
      grep -B 2 -A 2 "acquire_relate_guard" crates/memfuse-db/src/collection/relate.rs
- [ ] Confirm all error paths propagate (no let _ = )
- [ ] Run test gate manually:
      cargo test -p memfuse-db --test concurrent_collection_stress

## Code Quality (3 min)

- [ ] `cargo fmt --check` passes (CI enforces)
- [ ] No new unsafe code (verify AGENTS.md §4)
- [ ] FILE-CONTEXT headers updated
- [ ] RESOLVED tag on original AI-TAG

## Documentation (2 min)

- [ ] WORKING_STATE.md updated
- [ ] ADR reference in code comments
- [ ] CHANGELOG.md entry exists

## Sign-Off (1 min)

Once all checks pass:
```bash
# Leave REVIEW-PASS comment:
cat << 'EOF'
// REVIEW-PASS[1/3] Security Review Pass
// ID:       AGT-DB-a3f29c1d
// TS:       $(date -u +%Y-%m-%dT%H:%M:%SZ)
// SESSION:  <MY_SESSION_ID>
// STATUS:   PASS
// KONTEXT:  FRESH
// BEFUND:   RelateGuard correctly prevents race condition; all tests pass
EOF
```

Approve PR → Auto-requests 2 more reviews → Merges when 3 total.
```

---

## WORKFLOW 6: TROUBLESHOOTING

### Problem 1: Jules Fails to Load Context

**Symptom**: `session-context` returns no results

```bash
# Diagnostics
ls -la AGENTS.md WORKING_STATE.md .jules/SESSION_BOOTSTRAP.md
# → Wenn Dateien fehlen: Setup-Script ist nicht gelaufen

# Recovery
./env-setup.sh  # Re-run setup

# Manuell:
export JULIUS_SESSION_ID=$(date -u +%s | sha256sum | head -c 8)
cargo xtask context-digest --format text | head -5
```

### Problem 2: "Missing SESSION field" in AI-TAG

**Symptom**: `cargo xtask context-tags` skippt Tags ohne SESSION

```bash
# Ursache: Alte Tags (vor 2026-08-29) haben kein SESSION-Feld

# Fix: Manuelle Nachbearbeitung (einmalig)
find crates/ -name "*.rs" -exec grep -l "AI-TAG\[" {} \; | while read f; do
  sed -i 's/\(AI-TAG\[.*\]\[.*\] .*\)$/\1\n\/\/ SESSION: <LEGACY>/' "$f"
done

# Oder einfach: Neue Tags folgen neuem Format (Bestandsschutz)
```

### Problem 3: Tag Parsing Error

**Symptom**: `Parse error at relate.rs:123`

```bash
# Debug: Zeige raw Tag
sed -n '123,135p' crates/memfuse-db/src/collection/relate.rs

# Überprüfe Format:
# - Muss mit "// AI-TAG[" beginnen
# - Muss ID:, TS:, SESSION:, STATUS: haben
# - SESSION muss 8-stellige Hex sein

# Fix: Manuell korrigieren
```

### Problem 4: Multiple Sessions Modify Same File

**Symptom**: Git conflict in relate.rs

```bash
# Prevention (proaktiv):
git diff HEAD -- crates/memfuse-db/src/collection/relate.rs | grep "^+.*AI-TAG"
# → Zeigt neue Tags in diesem Branch

# Resolution: Koordinate mit anderen Sessions
cargo xtask context-file crates/memfuse-db/src/collection/relate.rs | grep "SESSION:"
# → Zeigt welche Sessions aktiv sind

# Merge strategy:
git merge --no-ff <other-branch>
just sync-docs  # Auto-resolves WORKING_STATE.md conflicts
```

---

## WORKFLOW 7: END-OF-SESSION CHECKLIST

Bevor Jules seine Sitzung beendet:

```bash
# ═══════════════════════════════════════════════════════════════════
# 1. Verify all tests green
# ═══════════════════════════════════════════════════════════════════
cargo test --workspace --exclude memfuse-tauri
# → All green ✓

# ═══════════════════════════════════════════════════════════════════
# 2. Format code
# ═══════════════════════════════════════════════════════════════════
cargo fmt --all

# ═══════════════════════════════════════════════════════════════════
# 3. Sync documentation (WICHTIG!)
# ═══════════════════════════════════════════════════════════════════
just sync-docs

# Output:
# [INFO] Syncing WORKING_STATE.md from inline tags...
# [INFO] Syncing CHANGELOG.md from AI-TAGs...
# [INFO] Syncing ARCHITECTURE.md from Cargo topology...
# [SUCCESS] All docs synced

# ═══════════════════════════════════════════════════════════════════
# 4. Verify sync was successful
# ═══════════════════════════════════════════════════════════════════
just sync-docs-check

# Output: [SUCCESS] All documentation is up-to-date

# ═══════════════════════════════════════════════════════════════════
# 5. DAG validation (wenn dependencies geändert)
# ═══════════════════════════════════════════════════════════════════
just dag-check

# ═══════════════════════════════════════════════════════════════════
# 6. Open ANCHORS Status
# ═══════════════════════════════════════════════════════════════════
cargo xtask context-tags --status OPEN | jq '.[] | select(.type == "ANCHOR")'
# → Sollte 0 sein (alle auf DONE/BLOCKED)

# ═══════════════════════════════════════════════════════════════════
# 7. Create PR (falls nicht schon geschehen)
# ═══════════════════════════════════════════════════════════════════
git add -A
git commit -m "Session <SESSION_ID>: Feature implementation + documentation sync

Summary:
- Fixed AGT-DB-a3f29c1d (relate() race condition)
- Implemented ANCHOR[INTEGRATION:WP-7.1] (Chunker integration)
- 2 REVIEW-PASS logged, awaiting 1 more
- All tests pass, docs synced"

git push origin feature/<branch>
# → GitHub creates PR, requires reviews

# ═══════════════════════════════════════════════════════════════════
# 8. Session Summary
# ═══════════════════════════════════════════════════════════════════
cat << 'EOF'
═══════════════════════════════════════════════════════════════════
✅ SESSION COMPLETE: a3f29c1d
═══════════════════════════════════════════════════════════════════
DURATION:        4 hours 32 min
TASKS COMPLETED: 2
  - Security: relate() race condition (CRITICAL)
  - Feature: Chunker integration (MAJOR)
TESTS PASSED:    ✓ 1847/1847
DOCS SYNCED:     ✓ WORKING_STATE.md, CHANGELOG.md, ARCHITECTURE.md
REVIEW PASS:     2/3 (awaiting final approval)
PR CREATED:      #1234 (https://github.com/.../pull/1234)
═══════════════════════════════════════════════════════════════════
EOF
```

---

## APPENDIX A: QUICK REFERENCE CARD

```
╔════════════════════════════════════════════════════════════════╗
║ JULES CONTEXT CLI — QUICK REFERENCE                           ║
╚════════════════════════════════════════════════════════════════╝

COMMON COMMANDS:
  jules-start                       # Init session (2 min)
  context-cli blockers              # Show critical issues
  context-cli file <PATH>           # File-specific context
  context-cli crate <CRATE>         # Crate overview
  context-cli tags --severity CRITICAL  # Filtered search

TESTING:
  just check                        # fmt + clippy + check
  cargo test --lib                  # Unit tests only
  cargo test --workspace            # Full test suite (15 min)
  just triple-test                  # Flaky detection

DOCUMENTATION:
  just sync-docs                    # Generate all docs
  just sync-docs-check              # Verify sync status

AUDIT:
  cargo xtask audit-verify <ID>     # Validate finding
  cargo xtask audit-review <ID>     # Log fix completion

END-OF-SESSION:
  cargo fmt --all                   # Auto-format
  just sync-docs                    # Update docs
  just sync-docs-check              # Verify
  git add -A && git commit           # Commit
  git push origin feature/<name>    # Push
```

---

**END OF PLAYBOOK**

Zielgruppe: Human Operators + Jules Agent Orchestration
Aktualisiert: 2026-08-29 · Version: 2.0
