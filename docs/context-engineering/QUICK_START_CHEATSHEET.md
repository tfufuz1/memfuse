# Jules Context System — Quick Start Cheat Sheet
**Schnelle Referenz für tägliche Nutzung**

Print this out or bookmark it! 📌

---

## SESSION STARTUP (5 min)

```bash
# 1. Setup
cd /app
./env-setup.sh
source ~/.bashrc

# 2. Quick init
jules-start

# 3. Show blockers
context-cli blockers | head -5
```

**Expected Output**:
```
✅ Session ID: a3f29c1d
✅ Ready to code
🚨 CRITICAL BLOCKERS: 1
  AGT-DB-a3f29c1d: Race condition in relate()
```

---

## COMMON QUERIES

### "Show me what's broken"
```bash
context-cli blockers
# → Lists all BLOCKER + CRITICAL issues
```

### "What's in memfuse-db?"
```bash
context-cli crate memfuse-db | head -30
# → Full crate context (AGENTS.md + structure + issues)
```

### "Find all CRITICAL issues in this crate"
```bash
cargo xtask context-tags --crate memfuse-db --severity CRITICAL
# → NDJSON output (grep-friendly)
```

### "Show me this file's context"
```bash
context-cli file crates/memfuse-db/src/collection/relate.rs
# → FILE-CONTEXT header + open issues + rustdoc
```

### "Find all related work (ANCHOR items)"
```bash
cargo xtask context-tags --type ANCHOR --status OPEN
# → Lists all open tasks
```

---

## FIXING AN ISSUE (Workflow)

### Step 1: Understand the problem
```bash
# Find the CRITICAL issue
cargo xtask context-tags --severity CRITICAL --status OPEN

# Get file context
context-cli file crates/memfuse-db/src/collection/relate.rs

# Check related ADRs
grep "ADR-023" DECISIONS.md | head -20
```

### Step 2: Implement fix
```bash
# Edit the file
vim crates/memfuse-db/src/collection/relate.rs

# Add AI-TAG RESOLVED line when done
// RESOLVED: AGT-DB-a3f29c1d — relate() now uses RelateGuard (TS: 2026-08-29T10:15:00Z)
```

### Step 3: Test
```bash
# Run gate
cargo test -p memfuse-db --test concurrent_collection_stress

# Full suite
cargo test --workspace --exclude memfuse-tauri
```

### Step 4: Finalize
```bash
# Format
cargo fmt --all

# Sync docs
just sync-docs

# Commit
git add -A
git commit -m "Fix: relate() race condition (AGT-DB-a3f29c1d)"
git push origin feature/fix-relate-race
```

---

## AUDIT FINDING (External)

### Validate it's still a problem
```bash
cargo xtask audit-verify AUDIT-2026-09-001 \
  --file crates/memfuse-index/src/hnsw.rs \
  --line 456

# Output: VALID ✓ or ALREADY_FIXED ✓ or FALSE_POSITIVE
```

### If VALID: Create tracking tag
```rust
// AI-TAG[MEMORY-SAFETY][CRITICAL] Integer overflow in vector allocation
// ID:       AGT-INDEX-b4e7f2c5
// TS:       2026-08-29T11:30:00Z
// SESSION:  a3f29c1d
// STATUS:   OPEN
// AUDIT_ID: AUDIT-2026-09-002
```

### Fix it
```bash
# ... implement fix ...
// RESOLVED: AUDIT-2026-09-002 — Integer overflow prevented (TS: 2026-08-29T12:00:00Z)
```

### Log review completion
```bash
cargo xtask audit-review AUDIT-2026-09-002 --status pass --note "Fix validated"
```

---

## PULL REQUEST CHECKLIST

Before pushing, verify:

```bash
# ✅ All tests pass
cargo test --workspace --exclude memfuse-tauri
✓ ALL TESTS PASSED

# ✅ Code is formatted
cargo fmt --all
✓ 1234 files formatted

# ✅ Docs are synced
just sync-docs
✓ WORKING_STATE.md updated

# ✅ No CRITICAL tags left open
context-cli blockers | grep -i open
(should be empty)

# ✅ RESOLVED tags on all fixed issues
grep -r "RESOLVED:" crates/ --include="*.rs"
(should show all your fixes)

# ✅ REVIEW-PASS logged (if security changes)
# Review-PASS[1/3] logged manually or auto-generated

# ✅ Ready to commit
git add -A
git commit -m "Title: Description + refs to AI-TAGs/ANCHORs"
git push origin feature/<name>
```

---

## TROUBLESHOOTING

### "context-cli not found"
```bash
source ~/.bashrc
# If still not found:
# → Run ./env-setup.sh again
```

### "Parse error: missing SESSION field"
```bash
# Your tag is from before 2026-08-29 (legacy)
# Add it manually:
// AI-TAG[...][...] ...
// SESSION: <your-session-id>
```

### "context-digest is slow"
```bash
# For large codebase, use filters:
cargo xtask context-digest --crate memfuse-db
# (vs. scanning all 15 crates)
```

### "Tests fail after my changes"
```bash
# Revert and check baseline
git stash
cargo test --lib
# If baseline passes, your change broke something
git stash pop
# Debug your change
```

### "Merge conflict in WORKING_STATE.md"
```bash
# Don't edit manually! Resolve via:
just sync-docs
git add WORKING_STATE.md
git commit
# (sync-docs regenerates from inline tags)
```

---

## COMMANDS BY USE CASE

### "I'm starting a new task"
```bash
jules-start                             # 1. Load context
context-cli crate <CRATE>              # 2. Understand scope
grep "ANCHOR\[" crates/**/*.rs         # 3. Find related work
```

### "I'm fixing a CRITICAL issue"
```bash
context-cli blockers                   # 1. See what's blocking
context-cli file <PATH>                # 2. Understand file
cargo xtask audit-verify <ID>         # 3. Validate (if audit)
# ... fix ...
cargo test --workspace                # 4. Verify
just sync-docs                         # 5. Sync docs
```

### "I'm reviewing someone's PR"
```bash
git checkout feature/<branch>
cargo test --workspace                 # 1. Check tests
cargo fmt --check                      # 2. Check formatting
context-cli file <MODIFIED_FILE>      # 3. Check tags
grep "RESOLVED:" crates/ --include="*.rs"  # 4. Verify fixes
```

### "I'm integrating an audit finding"
```bash
cargo xtask audit-verify AUDIT-XXXX   # 1. Validate
# ... implement fix ...
cargo xtask audit-review AUDIT-XXXX   # 2. Log completion
# ... create REVIEW-PASS entries ...
```

---

## ENVIRONMENT VARIABLES

```bash
# Set at session start (env-setup.sh does this)
export JULIUS_SESSION_ID=a3f29c1d     # 8-hex hash
export JULIUS_SESSION_START=2026-08-29T09:00:00Z

# Used in all AI-TAG fields (TS: and SESSION:)
# Check with:
echo $JULIUS_SESSION_ID
```

---

## USEFUL ALIASES (Add to ~/.bashrc)

```bash
# Quick checks
alias crit="context-cli blockers"
alias work="cargo xtask context-tags --status OPEN"
alias check="just check"

# Fast file context
alias ctx-file="context-cli file"
alias ctx-crate="context-cli crate"

# Testing shortcuts
alias t="cargo test --lib"
alias tt="cargo test --workspace --exclude memfuse-tauri"
alias t3="just triple-test"

# Doc sync
alias docs="just sync-docs"
alias docs-check="just sync-docs-check"

# One-liner session start
alias start-session="./env-setup.sh && source ~/.bashrc && jules-start"
```

---

## TIME ESTIMATES

| Task | Time |
|------|------|
| Session init (full) | 5–10 min |
| Fix small issue (MINOR) | 15–30 min |
| Fix CRITICAL issue | 30–60 min |
| Audit finding intake | 20–40 min |
| PR review (standard) | 15–20 min |
| Full test suite | 15 min |

---

## COMPLIANCE QUICK CHECKS

```bash
# Security audit findings processed?
cargo xtask audit-status | grep OPEN
# → Should be ~0

# All CRITICAL issues addressed?
context-cli blockers
# → Should be ~0 (might have MAJOR/MINOR)

# Documentation synced?
just sync-docs-check
# → Should show ✓

# Multi-session conflict risk?
git status
# → Should show no WORKING_STATE.md conflicts
```

---

## WHEN TO ESCALATE

**Escalate to Architecture Lead**:
- New public API changes
- Adding external dependencies
- Violating DAG constraints

**Escalate to Security Lead**:
- Any SECURITY category issue
- Multiple PANIC-SAFETY issues
- Unsafe code changes

**Escalate to Project Owner**:
- BLOCKER tags (blocks release)
- Audit findings (CRITICAL)
- 3+ REVIEW-PASS required

---

## LAST-MINUTE REMINDERS

✅ **Always**:
- Run `cargo test --workspace` before committing
- Run `cargo fmt --all` before committing
- Run `just sync-docs` before final commit
- Verify no BLOCKER/CRITICAL tags left open
- Add RESOLVED tags to fixed issues

❌ **Never**:
- Use `let _ =` for IO operations
- Use `SystemTime` for TxId generation
- Add unsafe code without asking
- Ignore .expect()/.unwrap() in production code
- Leave CRITICAL tags unaddressed

⚠️ **Be careful**:
- Multi-session file conflicts
- Missing SESSION field on new tags
- Parsing errors in AI-TAG format
- FILE-CONTEXT headers for large files

---

## QUICK REFERENCE: TAG FORMATS

### AI-TAG (Problem)
```rust
// AI-TAG[CATEGORY][SEVERITY] Short description
// ID:      AGT-<CRATE>-<hash>
// TS:      2026-08-29T...Z
// SESSION: <8-hex>
// STATUS:  OPEN
// BEFUND:  What's wrong
// RISIKO:  Why it matters
// EMPFEHLUNG: How to fix
```

### ANCHOR (Task)
```rust
// ANCHOR[TYPE:ID] Task description
// TS:     2026-08-29T...Z
// SESSION: <8-hex>
// STATUS:  OPEN
// GATE:    cargo test -p <crate>
```

### FILE-CONTEXT (Header)
```rust
// FILE-CONTEXT
// STAND:  2026-08-29T...Z (SESSION: <8-hex>)
// ZWECK:  One sentence what this file does
// SCOPE:  Crate: X | Layer: Y | Role: Z
// INVARIANTEN: Comma-separated must-haves
```

### REVIEW-PASS (Sign-off)
```rust
// REVIEW-PASS[1/3] Description
// ID:     AGT-<CRATE>-<hash>
// TS:     2026-08-29T...Z
// SESSION: <8-hex>
// STATUS:  PASS
// BEFUND:  What I verified
```

---

## EMERGENCY CONTACTS

**If you're stuck**:
1. Check JULES_OPERATORS_PLAYBOOK.md (Workflow 6: Troubleshooting)
2. Ask in #dev-jules Slack channel
3. Escalate to context-engineering@company.com

---

**Print Date**: 2026-08-29
**Version**: 2.0
**Status**: PRODUCTION

*Keep this card at your desk or browser bookmark! 📌*
