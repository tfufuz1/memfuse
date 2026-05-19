import os
import re
import sys
import subprocess
import json
from datetime import datetime, timedelta

# Configuration
STALE_THRESHOLD_HOURS = 8
# Path as specified in AGENT:00 requirements
INTEGRATE_SCRIPT_PATH = "/home/freddy/Arbeitsplatz/DEV/memfuse/.agent/scripts/jules-integrate.sh"
# Fallback to local path if absolute doesn't exist
LOCAL_INTEGRATE_PATH = ".agent/scripts/jules-integrate.sh"

def log(msg):
    print(f"[{datetime.now().isoformat()}] {msg}")

def get_now():
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

            return phase_2_deadlocks()

def phase_3_fv_gate():
    log("Phase 3: Auditing Formal Verification Gates...")
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
                    new_lines.append("// WATCHDOG: Blocking merges due to missing Kani/TLA+ proofs for REVIEW components (WAL/LSM).\n")
                    line = re.sub(r"STATUS:[A-Z]+", "STATUS:OPEN", line)
                    changed = True
            new_lines.append(line)

        if changed:
            with open(gate_path, "w") as f:
                f.writelines(new_lines)

def phase_4_pr_integration():
    log("Phase 4: Checking PR Integration...")
    if subprocess.run(["which", "gh"], capture_output=True).returncode != 0:
        log("gh CLI not found. Skipping PR integration.")
        return

    try:
        cmd = ["gh", "pr", "list", "--label", "jules", "--json", "number,statusCheckRollup,mergeable"]
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            log(f"gh command failed: {result.stderr}")
            return

        prs = json.loads(result.stdout)
        eligible_count = 0
        for pr in prs:
            rollup = pr.get('statusCheckRollup', [])
            all_passed = len(rollup) > 0 and all(c.get('conclusion') == 'SUCCESS' or c.get('status') == 'COMPLETED' for c in rollup)
            has_failures = any(c.get('conclusion') in ['FAILURE', 'CANCELLED', 'TIMED_OUT'] for c in rollup)

            if pr['mergeable'] == 'MERGEABLE' and all_passed and not has_failures:
                eligible_count += 1

        if eligible_count > 0:
            log(f"Found {eligible_count} PRs ready for integration. Calling integration script...")
            script = INTEGRATE_SCRIPT_PATH if os.path.exists(INTEGRATE_SCRIPT_PATH) else LOCAL_INTEGRATE_PATH
            if os.path.exists(script):
                res = subprocess.run(["bash", script], capture_output=True, text=True)
                if res.returncode == 0:
                    log("✅ PR Integration sequence completed successfully.")
                else:
                    log(f"❌ PR Integration script failed (exit {res.returncode})")
            else:
                log(f"Integration script missing at: {script}")
        else:
            log("No PRs currently eligible for integration.")

    except Exception as e:
        log(f"PR monitoring failed: {e}")

def phase_5_stability_audit():
    log("Phase 5: Workspace Stability Audit...")
    result = subprocess.run(["cargo", "check", "--workspace"], capture_output=True, text=True)
    if result.returncode != 0:
        log("❌ Workspace is UNSTABLE.")
    else:
        log("✅ Workspace is STABLE.")

if __name__ == "__main__":
    phase_1_stale_wip()
    phase_2_deadlocks()
    phase_3_fv_gate()
    phase_4_pr_integration()
    phase_5_stability_audit()
