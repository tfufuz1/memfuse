#!/usr/bin/env python3
import os
import re
import datetime
import sys

# Configuration
PROJECT_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
CURRENT_DATE = datetime.date.today()
# Note: Anchors only have DATE (YYYY-MM-DD). We treat any date before today as > 8 hours.
# To properly support 8h timeout, the anchor format would need a timestamp.

# Regex patterns
ANCHOR_RE = re.compile(r"//\s*ANCHOR:(?P<type>[^:\s]+)(:(?P<id>[^\s]+))?\s+AGENT:(?P<agent>[^\s]+)\s+DATE:(?P<date>\d{4}-\d{2}-\d{2})\s+STATUS:(?P<status>[^\s]+)")
WP_RE = re.compile(r"//\s*WP:(?P<wp>[^\s]+)\s+PRIO:(?P<prio>\d+)\s+NEEDS:(?P<needs>[^\s]+)")

def get_files():
    files = []
    for root, _, filenames in os.walk(PROJECT_ROOT):
        if ".git" in root or "target" in root:
            continue
        for filename in filenames:
            if filename.endswith((".rs", ".md", ".toml")):
                files.append(os.path.join(root, filename))
    return files

def phase1_stale_anchors(files):
    print("--- Phase 1: Stale WIP Anchors ---")
    modified_count = 0
    for filepath in files:
        with open(filepath, 'r') as f:
            content = f.read()

        lines = content.splitlines()
        modified = False
        for i, line in enumerate(lines):
            match = ANCHOR_RE.search(line)
            if match:
                status = match.group("status")
                if status in ["WIP", "ACTIVE"]:
                    try:
                        date_str = match.group("date")
                        anchor_date = datetime.datetime.strptime(date_str, "%Y-%m-%d").date()
                        if anchor_date < CURRENT_DATE:
                            print(f"Resetting stale anchor in {filepath}:{i+1}")
                            lines[i] = ANCHOR_RE.sub(lambda m: m.group(0).replace(status, "OPEN"), line)
                            if i == 0 or "// WATCHDOG" not in lines[i-1]:
                                lines.insert(i, "// WATCHDOG: Reset WIP due to timeout.")
                            modified = True
                            modified_count += 1
                    except ValueError:
                        pass

        if modified:
            with open(filepath, 'w') as f:
                f.write("\n".join(lines) + "\n")
    print(f"Modified {modified_count} stale anchors.")

def phase2_deadlocks(files):
    print("--- Phase 2: Cross-Agent Deadlocks ---")
    anchors = []
    for filepath in files:
        with open(filepath, 'r') as f:
            content = f.read()

        lines = content.splitlines()
        for i, line in enumerate(lines):
            a_match = ANCHOR_RE.search(line)
            if a_match:
                needs = "NONE"
                # Search for associated WP within 5 lines
                for j in range(max(0, i-5), min(len(lines), i+6)):
                    w_match = WP_RE.search(lines[j])
                    if w_match:
                        needs = w_match.group("needs")
                        break

                anchors.append({
                    "filepath": filepath,
                    "line_idx": i,
                    "id": a_match.group("id") or a_match.group("type"),
                    "status": a_match.group("status"),
                    "needs": needs.split("+") if needs != "NONE" else []
                })

    blocked = [a for a in anchors if a["status"] == "BLOCKED"]
    if not blocked:
        print("No BLOCKED anchors found.")
        return

    def find_cycle(node_id, visited, stack, path):
        visited.add(node_id)
        stack.add(node_id)
        path.append(node_id)

        current_anchor = next((a for a in anchors if a["id"] == node_id), None)
        if current_anchor:
            for dep in current_anchor["needs"]:
                if dep not in visited:
                    if find_cycle(dep, visited, stack, path):
                        return True
                elif dep in stack:
                    path.append(dep)
                    return True

        stack.remove(node_id)
        path.pop()
        return False

    visited = set()
    for b in blocked:
        path = []
        if b["id"] not in visited:
            if find_cycle(b["id"], visited, set(), path):
                print(f"Cycle detected: {' -> '.join(path)}")
                target_id = path[0]
                target_anchor = next((a for a in anchors if a["id"] == target_id), None)
                if target_anchor:
                    print(f"Breaking cycle at {target_id} in {target_anchor['filepath']}")
                    with open(target_anchor["filepath"], 'r') as f:
                        lines = f.read().splitlines()

                    lines[target_anchor["line_idx"]] = lines[target_anchor["line_idx"]].replace("STATUS:BLOCKED", "STATUS:OPEN")
                    for j in range(max(0, target_anchor["line_idx"]-5), min(len(lines), target_anchor["line_idx"]+6)):
                        if "NEEDS:" in lines[j]:
                            lines[j] = re.sub(r"NEEDS:[^\s]+", "NEEDS:NONE", lines[j])

                    if target_anchor["line_idx"] == 0 or "// WATCHDOG" not in lines[target_anchor["line_idx"]-1]:
                        lines.insert(target_anchor["line_idx"], "// WATCHDOG: Broken cyclic dependency.")

                    with open(target_anchor["filepath"], 'w') as f:
                        f.write("\n".join(lines) + "\n")

                    # Update in-memory state
                    target_anchor["status"] = "OPEN"
                    target_anchor["needs"] = []

def phase3_fv_gate(files):
    print("--- Phase 3: Formal Verification Gates ---")
    # Crates requiring FV for critical paths
    CRITICAL_CRATES = ["memfuse-store", "memfuse-db", "memfuse-crypto"]

    crate_status = {crate: {"review": False, "verified": False} for crate in CRITICAL_CRATES}

    for filepath in files:
        for crate in CRITICAL_CRATES:
            if f"crates/{crate}/" in filepath:
                with open(filepath, 'r') as f:
                    content = f.read()
                    if "STATUS:REVIEW" in content:
                        crate_status[crate]["review"] = True
                    if "kani::proof" in content:
                        crate_status[crate]["verified"] = True

    missing_fv = []
    for crate, status in crate_status.items():
        if status["review"] and not status["verified"]:
            missing_fv.append(crate)

    gate_file = os.path.join(PROJECT_ROOT, "crates/memfuse-core/src/lib.rs")
    if missing_fv:
        print(f"Missing formal verification for REVIEW components in: {', '.join(missing_fv)}")
        if os.path.exists(gate_file):
            with open(gate_file, 'r') as f:
                lines = f.read().splitlines()

            modified = False
            for i, line in enumerate(lines):
                if "ANCHOR:ARCH:GATE-FV" in line and "STATUS:OPEN" not in line:
                    lines[i] = line.replace("STATUS:DONE", "STATUS:OPEN").replace("STATUS:READY", "STATUS:OPEN")
                    if i == 0 or "// WATCHDOG" not in lines[i-1]:
                        lines.insert(i, f"// WATCHDOG: Blocking merges due to missing Kani/TLA+ proofs in: {', '.join(missing_fv)}")
                    modified = True

            if modified:
                print("Setting ARCH:GATE-FV to OPEN.")
                with open(gate_file, 'w') as f:
                    f.write("\n".join(lines) + "\n")
    else:
        print("FV Gate requirements met or no REVIEW components found.")

if __name__ == "__main__":
    files = get_files()
    phase1_stale_anchors(files)
    phase2_deadlocks(files)
    phase3_fv_gate(files)
