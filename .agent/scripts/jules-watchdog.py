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
    # Use environment variable for testing/simulation if provided
    env_now = os.getenv("WATCHDOG_NOW")
    if env_now:
        try:
            return datetime.fromisoformat(env_now)
        except ValueError:
            pass
    return datetime.now()

def parse_date(date_str):
    for fmt in ("%Y-%m-%d", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"):
        try:
            return datetime.strptime(date_str, fmt)
        except ValueError:
            continue
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
                    date_match = re.search(r"(?:DATE|WIP-START|CREATED):([\d\-\s:T]+)", line)
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

            matches = re.finditer(r"// ANCHOR:([A-Z0-9_-]+):?([A-Z0-9_-]+)?.*", content)
            for m in matches:
                full_match = m.group(0)
                id_match = re.search(r"ANCHOR:([A-Z0-9_-]+)(?::([A-Z0-9_-]+))?", full_match)
                if not id_match: continue
                anchor_id = id_match.group(2) if id_match.group(2) else id_match.group(1)

                status_match = re.search(r"STATUS:([A-Z]+)", full_match)
                status = status_match.group(1) if status_match else "UNKNOWN"

                needs_match = re.search(r"(?:NEEDS|DEPS|NEEDS):([A-Z0-9_-]+(?:,[A-Z0-9_-]+)*)", full_match)
                deps = needs_match.group(1).split(",") if needs_match else []
                deps = [d.strip() for d in deps if d.strip() not in ["NONE", "DONE"]]

                anchors[anchor_id] = {
                    "deps": deps,
                    "path": path,
                    "status": status,
                    "full_match": full_match
                }

    def find_all_cycles():
        visited = set()
        stack = []
        cycles = []

        def dfs(u):
            visited.add(u)
            stack.append(u)
            for v in anchors.get(u, {}).get("deps", []):
                if v in stack:
                    cycles.append(stack[stack.index(v):])
                elif v not in visited:
                    dfs(v)
            stack.pop()

        for u in anchors:
            if u not in visited:
                dfs(u)
        return cycles

    cycles = find_all_cycles()
    if cycles:
        for cycle in cycles:
            log(f"DETECTED CYCLE: {' -> '.join(cycle)}")
            # Pick simplest node: one with minimum dependencies
            target_id = min(cycle, key=lambda x: len(anchors[x]["deps"]))
            target_path = anchors[target_id]["path"]

            log(f"Resolving cycle by resetting {target_id} in {target_path}")
            with open(target_path, "r") as f:
                lines = f.readlines()

            new_lines = []
            for line in lines:
                if f"ANCHOR" in line and target_id in line:
                    new_lines.append("// WATCHDOG: Broken cyclic dependency.\n")
                    line = re.sub(r"(?:NEEDS|DEPS|NEEDS):[A-Z0-9_-]+(?:,[A-Z0-9_-]+)*", "NEEDS:NONE", line)
                    line = line.replace("STATUS:BLOCKED", "STATUS:OPEN")
                new_lines.append(line)

            with open(target_path, "w") as f:
                f.writelines(new_lines)

            # Re-build and re-scan for more cycles
            return phase_2_deadlocks()

def phase_3_fv_gate():
    log("Phase 3: Auditing Formal Verification Gates...")
    critical_components = ["WAL", "LSM", "Encryption", "Crypto", "Storage", "SSTable"]
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
                is_critical = any(comp.lower() in path.lower() or comp in content for comp in critical_components)
                if is_critical and "#[kani::proof]" not in content:
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
                is_open = "STATUS:OPEN" in line
                if missing_proofs and not is_open:
                    log("Enforcing Formal Verification Gate: STATUS:OPEN")
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
    if not os.path.exists(script_path):
        log(f"Integration script {script_path} not found.")
        return

    # Call the script which handles GH CLI calls and filtering
    log(f"Executing integration script: {script_path}")
    try:
        result = subprocess.run(["bash", script_path], capture_output=True, text=True)
        if result.returncode == 0:
            log("PR Integration step completed successfully.")
            if result.stdout.strip():
                print(result.stdout)
        else:
            log(f"PR Integration step failed (exit code {result.returncode}).")
            if "gh: command not found" in result.stderr:
                log("Reason: 'gh' CLI tool is not installed.")
    except Exception as e:
        log(f"Error executing integration script: {e}")

if __name__ == "__main__":
    phase_1_stale_wip()
    phase_2_deadlocks()
    phase_3_fv_gate()
    phase_4_pr_integration()
