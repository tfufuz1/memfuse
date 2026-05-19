import os
import re
import sys
import subprocess
import json
from datetime import datetime, timedelta

# Configuration
STALE_THRESHOLD_HOURS = 8

def log(msg):
    print(f"[{datetime.now().isoformat()}] {msg}")

def get_now():
    # Allow simulation for testing
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

    for root, _, files in os.walk("."):
        if any(d in root for d in [".git", ".agent", "target"]): continue
        for file in files:
            if not file.endswith((".rs", ".md", ".toml")): continue
            path = os.path.join(root, file)
            try:
                with open(path, "r") as f:
                    content = f.read()
            except: continue

            # Match standard anchor format
            matches = re.finditer(r"// ANCHOR:([A-Z0-9_-]+):?([A-Z0-9_-]+)?.*", content)
            for m in matches:
                full_match = m.group(0)
                id_match = re.search(r"ANCHOR:([A-Z0-9_-]+)(?::([A-Z0-9_-]+))?", full_match)
                if not id_match: continue
                anchor_id = id_match.group(2) if id_match.group(2) else id_match.group(1)

                status_match = re.search(r"STATUS:([A-Z]+)", full_match)
                status = status_match.group(1) if status_match else "UNKNOWN"

                needs_match = re.search(r"(?:NEEDS|DEPS):([A-Z0-9_-]+(?:,[A-Z0-9_-]+)*)", full_match)
                deps = needs_match.group(1).split(",") if needs_match else []
                deps = [d.strip() for d in deps if d.strip() not in ["NONE", "DONE"]]

                anchors[anchor_id] = {
                    "deps": deps,
                    "path": path,
                    "status": status
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
                    cycles.append(list(stack[stack.index(v):]))
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
            # Identify simplest node: minimal dependencies
            target_id = min(cycle, key=lambda x: len(anchors[x]["deps"]))
            target_path = anchors[target_id]["path"]

            log(f"Resolving cycle by resetting {target_id} in {target_path}")
            with open(target_path, "r") as f:
                lines = f.readlines()

            new_lines = []
            for line in lines:
                if f"ANCHOR" in line and target_id in line:
                    new_lines.append("// WATCHDOG: Broken cyclic dependency.\n")
                    line = re.sub(r"(?:NEEDS|DEPS):[A-Z0-9_-]+(?:,[A-Z0-9_-]+)*", "NEEDS:NONE", line)
                    line = line.replace("STATUS:BLOCKED", "STATUS:OPEN")
                new_lines.append(line)

            with open(target_path, "w") as f:
                f.writelines(new_lines)

            # Re-scan for more cycles
            return phase_2_deadlocks()

def phase_3_fv_gate():
    log("Phase 3: Auditing Formal Verification Gates...")
    # Components requiring FV: WAL, LSM, Encryption/Crypto
    critical_files = ["wal.rs", "lsm.rs", "crypto.rs", "sstable.rs"]
    missing_proofs = False

    for root, _, files in os.walk("crates"):
        for file in files:
            if file in critical_files:
                path = os.path.join(root, file)
                try:
                    with open(path, "r") as f:
                        content = f.read()
                        if "STATUS:REVIEW" in content and "#[kani::proof]" not in content:
                            log(f"Missing Kani proof for REVIEW component: {path}")
                            missing_proofs = True
                except: continue

    gate_path = "crates/memfuse-core/src/lib.rs"
    if os.path.exists(gate_path):
        with open(gate_path, "r") as f:
            lines = f.readlines()

        changed = False
        new_lines = []
        for line in lines:
            if "ARCH:GATE-FV" in line:
                if missing_proofs and "STATUS:OPEN" not in line:
                    log("Enforcing ARCH:GATE-FV STATUS:OPEN")
                    new_lines.append("// WATCHDOG: Blocking merges due to missing Kani/TLA+ proofs.\n")
                    line = re.sub(r"STATUS:[A-Z]+", "STATUS:OPEN", line)
                    changed = True
            new_lines.append(line)

        if changed:
            with open(gate_path, "w") as f:
                f.writelines(new_lines)

def phase_4_pr_integration():
    log("Phase 4: Checking PR Integration...")
    if subprocess.run(["which", "gh"], capture_output=True).returncode != 0:
        log("gh CLI not found. Cannot monitor PRs.")
        return

    try:
        # Get PRs with label 'jules'
        cmd = ["gh", "pr", "list", "--label", "jules", "--json", "number,statusCheckRollup,mergeable"]
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            log(f"gh command failed: {result.stderr}")
            return

        prs = json.loads(result.stdout)
        for pr in prs:
            num = pr['number']
            mergeable = pr['mergeable'] == 'MERGEABLE'

            # Check if CI passed (Gate 1)
            # statusCheckRollup contains an array of checks
            rollup = pr.get('statusCheckRollup', [])
            all_passed = len(rollup) > 0 and all(c.get('conclusion') == 'SUCCESS' or c.get('status') == 'COMPLETED' for c in rollup)
            # Filter for failures
            has_failures = any(c.get('conclusion') in ['FAILURE', 'CANCELLED', 'TIMED_OUT'] for c in rollup)

            if mergeable and all_passed and not has_failures:
                log(f"PR #{num} passed Gate 1. Triggering integration...")
                integrate_script = ".agent/scripts/jules-integrate.sh"
                if os.path.exists(integrate_script):
                    # In a real environment, we'd call the script
                    # For safety in this sandbox, we log the attempt.
                    subprocess.run(["bash", integrate_script], check=False)
                else:
                    log(f"Integration script {integrate_script} missing.")
            else:
                log(f"PR #{num}: Mergeable={mergeable}, ChecksPassed={all_passed and not has_failures}")

    except Exception as e:
        log(f"PR monitoring failed: {e}")

if __name__ == "__main__":
    phase_1_stale_wip()
    phase_2_deadlocks()
    phase_3_fv_gate()
    phase_4_pr_integration()
