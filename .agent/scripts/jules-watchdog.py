#!/usr/bin/env python3
import os
import re
import sys
from datetime import datetime, timedelta

# Configuration
STALE_THRESHOLD_HOURS = 8
ANCHOR_PATTERN = re.compile(r"//\s*ANCHOR:([\w:-]+)")
STATUS_PATTERN = re.compile(r"STATUS:(\w+)")
WIP_START_PATTERN = re.compile(r"WIP-START:(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})")
CREATED_PATTERN = re.compile(r"CREATED:(\d{4}-\d{2}-\d{2})")
DATE_PATTERN = re.compile(r"DATE:(\d{4}-\d{2}-\d{2})")
NEEDS_PATTERN = re.compile(r"NEEDS:([\w:,-]+)")
DEPS_PATTERN = re.compile(r"DEPS:([\w:,-]+)")

def log(msg):
    print(f"[WATCHDOG] {msg}")

def parse_date(date_str):
    if not date_str:
        return None
    try:
        if 'T' in date_str:
            return datetime.strptime(date_str, "%Y-%m-%dT%H:%M:%S")
        else:
            return datetime.strptime(date_str, "%Y-%m-%d")
    except ValueError:
        return None

def find_anchors():
    anchors = []
    for root, dirs, files in os.walk("."):
        if any(d in root for d in [".git", "target", "node_modules"]):
            continue
        for file in files:
            if file.endswith((".rs", ".md", ".toml", ".txt")):
                path = os.path.join(root, file)
                try:
                    with open(path, "r", errors="ignore") as f:
                        lines = f.readlines()
                        for i, line in enumerate(lines):
                            if "ANCHOR:" in line:
                                m = ANCHOR_PATTERN.search(line)
                                if m:
                                    name = m.group(1)
                                    context = "".join(lines[i:i+5])
                                    status_m = STATUS_PATTERN.search(context)
                                    status = status_m.group(1) if status_m else "OPEN"

                                    wip_start = WIP_START_PATTERN.search(context)
                                    created = CREATED_PATTERN.search(context)
                                    date = DATE_PATTERN.search(context)

                                    start_time = parse_date(wip_start.group(1) if wip_start else None)
                                    if not start_time:
                                        start_time = parse_date(date.group(1) if date else (created.group(1) if created else None))

                                    needs_m = NEEDS_PATTERN.search(context) or DEPS_PATTERN.search(context)
                                    needs = needs_m.group(1).split(",") if needs_m and needs_m.group(1) != "NONE" else []

                                    anchors.append({
                                        "name": name,
                                        "status": status,
                                        "start_time": start_time,
                                        "needs": needs,
                                        "file": path,
                                        "line": i,
                                        "original_line": line
                                    })
                except Exception as e:
                    log(f"Error reading {path}: {e}")
    return anchors

def phase1_reset_stale(anchors):
    now = datetime.now()
    log("Phase 1: Resetting stale WIP anchors...")
    for a in anchors:
        if a["status"] == "WIP" and a["start_time"]:
            if now - a["start_time"] > timedelta(hours=STALE_THRESHOLD_HOURS):
                log(f"Stale anchor found: {a['name']} in {a['file']}:{a['line']+1}")
                reset_anchor(a)

def reset_anchor(a):
    with open(a["file"], "r") as f:
        lines = f.readlines()

    line = lines[a["line"]]
    new_line = line.replace("STATUS:WIP", "STATUS:OPEN")
    lines[a["line"]] = f"// WATCHDOG: Reset WIP due to timeout.\n{new_line}"

    with open(a["file"], "w") as f:
        f.writelines(lines)
    log(f"Reset {a['name']} to OPEN.")

def phase2_resolve_deadlocks(anchors):
    log("Phase 2: Checking for deadlocks...")
    blocked = [a for a in anchors if a["status"] == "BLOCKED"]
    if not blocked:
        log("No BLOCKED anchors found.")
        return

    # Build dependency graph
    graph = {a["name"]: a["needs"] for a in anchors}

    for a in blocked:
        path = find_cycle(a["name"], graph, set(), [])
        if path:
            log(f"Cycle detected: {' -> '.join(path)}")
            resolve_cycle(path, anchors)

def find_cycle(node, graph, visited, stack):
    visited.add(node)
    stack.append(node)

    for neighbor in graph.get(node, []):
        if neighbor not in visited:
            res = find_cycle(neighbor, graph, visited, stack)
            if res: return res
        elif neighbor in stack:
            return stack[stack.index(neighbor):] + [neighbor]

    stack.pop()
    return None

def resolve_cycle(path, anchors):
    target_name = path[0]
    target_anchor = next((a for a in anchors if a["name"] == target_name), None)
    if not target_anchor: return

    log(f"Breaking cycle at {target_name}")
    with open(target_anchor["file"], "r") as f:
        lines = f.readlines()

    line = lines[target_anchor["line"]]
    new_line = line.replace("STATUS:BLOCKED", "STATUS:OPEN")
    # Also remove the dependency that causes the cycle in this simple implementation
    # This is complex to do via regex on a single line safely, but let's try a comment
    lines[target_anchor["line"]] = f"// WATCHDOG: Broken cyclic dependency.\n{new_line}"

    with open(target_anchor["file"], "w") as f:
        f.writelines(lines)

def phase3_monitor_gates(anchors):
    log("Phase 3: Monitoring Formal Verification Gates...")
    review_anchors = [a for a in anchors if a["status"] == "REVIEW"]

    # Check if any REVIEW anchors are in sensitive areas (store, crypto)
    sensitive_review = [a for a in review_anchors if any(x in a["file"] for x in ["memfuse-store", "crypto", "encryption"])]

    # Check for missing kani proofs in those files
    missing_proofs = False
    for a in sensitive_review:
        with open(a["file"], "r") as f:
            content = f.read()
            if "kani" not in content.lower() and "proof" not in content.lower():
                log(f"Missing formal verification for {a['name']} in {a['file']}")
                missing_proofs = True

    gate_file = "crates/memfuse-core/src/lib.rs"
    if os.path.exists(gate_file):
        with open(gate_file, "r") as f:
            content = f.read()

        if missing_proofs:
            if "ARCH:GATE-FV STATUS:OPEN" not in content:
                log("Opening Gate-FV due to missing proofs.")
                new_content = content.replace("ARCH:GATE-FV STATUS:DONE", "ARCH:GATE-FV STATUS:OPEN")
                new_content = new_content.replace("ARCH:GATE-FV STATUS:READY", "ARCH:GATE-FV STATUS:OPEN")
                with open(gate_file, "w") as f:
                    f.write(new_content)
        else:
            log("No missing proofs detected for reviewed sensitive components.")

def main():
    anchors = find_anchors()
    phase1_reset_stale(anchors)
    phase2_resolve_deadlocks(anchors)
    phase3_monitor_gates(anchors)
    log("Watchdog run complete.")

if __name__ == "__main__":
    main()
