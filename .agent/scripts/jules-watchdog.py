import os
import re
import sys
import subprocess
from datetime import datetime, timedelta

# Configuration
STALE_THRESHOLD_HOURS = 8

def log(msg):
    print(f"[{datetime.now().isoformat()}] {msg}")

def get_now():
    # In a real system this would be datetime.now()
    # For this task, we assume the system time is 2026-05-19 22:30
    return datetime(2026, 5, 19, 22, 30)

def parse_date(date_str):
    try:
        return datetime.strptime(date_str, "%Y-%m-%d")
    except ValueError:
        try:
            return datetime.strptime(date_str, "%Y-%m-%d %H:%M")
        except ValueError:
            return None

def phase_1_stale_wip():
    log("Phase 1: Scanning for stale WIP anchors...")
    now = get_now()
    for root, _, files in os.walk("."):
        if any(d in root for d in [".git", ".agent", "target"]): continue
        for file in files:
            if not file.endswith((".rs", ".md", ".toml")): continue
            path = os.path.join(root, file)
            try:
                with open(path, "r") as f:
                    lines = f.readlines()
            except: continue

            changed = False
            new_lines = []
            for i, line in enumerate(lines):
                if "STATUS:WIP" in line:
                    # Look for DATE: or WIP-START: or CREATED:
                    date_match = re.search(r"(?:DATE|WIP-START|CREATED):([\d\-\s:]+)", line)
                    if date_match:
                        anchor_date = parse_date(date_match.group(1).strip())
                        if anchor_date and (now - anchor_date) > timedelta(hours=STALE_THRESHOLD_HOURS):
                            log(f"Resetting stale WIP at {path}:{i+1}")
                            new_lines.append("// WATCHDOG: Reset WIP due to timeout.\n")
                            line = line.replace("STATUS:WIP", "STATUS:OPEN")
                            changed = True
                new_lines.append(line)

            if changed:
                with open(path, "w") as f:
                    f.writelines(new_lines)

def phase_2_deadlocks():
    log("Phase 2: Scanning for cross-agent deadlocks...")
    anchors = {}

    # Pass 1: Build dependency graph
    for root, _, files in os.walk("."):
        if any(d in root for d in [".git", ".agent", "target"]): continue
        for file in files:
            if not file.endswith((".rs", ".md", ".toml")): continue
            path = os.path.join(root, file)
            try:
                with open(path, "r") as f:
                    content = f.read()
            except: continue

            # Find anchors: // ANCHOR:[TYPE]:[ID] ... NEEDS:[DEPS] ... STATUS:[STATUS]
            matches = re.finditer(r"// ANCHOR:([A-Z0-9_-]+):?([A-Z0-9_-]+)?.*", content)
            for m in matches:
                full_match = m.group(0)
                # Try to extract ID
                id_match = re.search(r"ANCHOR:([A-Z0-9_-]+)(?::([A-Z0-9_-]+))?", full_match)
                if not id_match: continue
                anchor_id = id_match.group(2) if id_match.group(2) else id_match.group(1)

                status_match = re.search(r"STATUS:([A-Z]+)", full_match)
                status = status_match.group(1) if status_match else "UNKNOWN"

                needs_match = re.search(r"(?:NEEDS|DEPS):([A-Z0-9_-]+(?:,[A-Z0-9_-]+)*)", full_match)
                deps = needs_match.group(1).split(",") if needs_match else []
                deps = [d for d in deps if d not in ["NONE", "DONE"]]

                anchors[anchor_id] = {
                    "deps": deps,
                    "path": path,
                    "status": status,
                    "line_content": full_match
                }

    def find_cycle(curr_id, visited, stack, path_trace):
        visited.add(curr_id)
        stack.add(curr_id)
        path_trace.append(curr_id)

        for dep in anchors.get(curr_id, {}).get("deps", []):
            if dep not in visited:
                cycle = find_cycle(dep, visited, stack, path_trace)
                if cycle: return cycle
            elif dep in stack:
                return path_trace + [dep]

        stack.remove(curr_id)
        path_trace.pop()
        return None

    visited = set()
    for aid in list(anchors.keys()):
        if aid not in visited:
            cycle = find_cycle(aid, visited, set(), [])
            if cycle:
                log(f"Detected circular dependency: {' -> '.join(cycle)}")
                # Resolve by picking the first node in cycle that is BLOCKED
                target_id = None
                for cid in cycle:
                    if anchors.get(cid, {}).get("status") == "BLOCKED":
                        target_id = cid
                        break

                if not target_id: target_id = cycle[0]

                target_path = anchors[target_id]["path"]
                log(f"Resolving cycle by resetting {target_id} in {target_path}")

                with open(target_path, "r") as f:
                    lines = f.readlines()

                new_lines = []
                for line in lines:
                    if target_id in line and "ANCHOR" in line:
                        new_lines.append("// WATCHDOG: Broken cyclic dependency.\n")
                        line = re.sub(r"(?:NEEDS|DEPS):[A-Z0-9_-]+(?:,[A-Z0-9_-]+)*", "NEEDS:NONE", line)
                        line = line.replace("STATUS:BLOCKED", "STATUS:OPEN")
                    new_lines.append(line)

                with open(target_path, "w") as f:
                    f.writelines(new_lines)

                # Refresh anchors and restart scan to find more cycles safely
                return phase_2_deadlocks()

def phase_3_fv_gate():
    log("Phase 3: Auditing Formal Verification Gates...")
    critical_components = ["WAL", "LSM", "Encryption", "Crypto", "Storage"]
    missing_proofs = False

    for root, _, files in os.walk("crates"):
        for file in files:
            if not file.endswith(".rs"): continue
            path = os.path.join(root, file)
            try:
                with open(path, "r") as f:
                    content = f.read()
            except: continue

            if "STATUS:REVIEW" in content:
                # Is it a critical component?
                is_critical = any(comp.lower() in path.lower() or comp in content for comp in critical_components)
                if is_critical:
                    # Check for Kani proof
                    if "#[kani::proof]" not in content:
                        log(f"Missing Kani proof for REVIEW component at {path}")
                        missing_proofs = True

    gate_file = "crates/memfuse-core/src/lib.rs"
    if os.path.exists(gate_file):
        with open(gate_file, "r") as f:
            lines = f.readlines()

        changed = False
        new_lines = []
        for line in lines:
            if "ARCH:GATE-FV" in line:
                current_status = "OPEN" if "STATUS:OPEN" in line else "CLOSED"
                if missing_proofs and current_status == "CLOSED":
                    log("Opening Formal Verification Gate due to missing proofs.")
                    new_lines.append("// WATCHDOG: Blocking merges due to missing Kani/TLA+ proofs for REVIEW components.\n")
                    line = line.replace("STATUS:DONE", "STATUS:OPEN").replace("STATUS:REVIEW", "STATUS:OPEN")
                    changed = True
            new_lines.append(line)

        if changed:
            with open(gate_file, "w") as f:
                f.writelines(new_lines)

def phase_4_pr_integration():
    log("Phase 4: GitHub PR Integration...")
    script_path = ".agent/scripts/jules-integrate.sh"
    if os.path.exists(script_path):
        log(f"Attempting to call {script_path}")
        result = subprocess.run(["bash", script_path], capture_output=True, text=True)
        if result.returncode == 0:
            log("PR Integration successful.")
            print(result.stdout)
        else:
            log("PR Integration failed or skipped (e.g. missing 'gh' CLI).")

if __name__ == "__main__":
    phase_1_stale_wip()
    phase_2_deadlocks()
    phase_3_fv_gate()
    phase_4_pr_integration()
