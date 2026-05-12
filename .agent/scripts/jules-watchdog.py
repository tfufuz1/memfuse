import os
import re
import subprocess
import json
from datetime import datetime, timedelta

# Doctrine: AGENT:00 Orchestrator-Watchdog
# Tasks: Reset stale anchors, Resolve deadlocks, Monitor FV Gates, PR Integration.

REPO_ROOT = os.getcwd()
STALE_THRESHOLD = timedelta(hours=8)

ANCHOR_RE = re.compile(r"ANCHOR:([\w:.-]+)")
KV_RE = re.compile(r"(\bWP|\bPRIO|\bNEEDS|\bAGENT|\bDATE|\bCREATED|\bSTATUS|\bDEADLINE|\bSUCCESSOR|\bDEPS):")

def get_now():
    # Attempt to get time from system, default to a safe baseline if needed
    return datetime.now()

def parse_date(date_str):
    if not date_str: return None
    date_str = date_str.strip().replace("[HEUTE]", datetime.now().strftime("%Y-%m-%d"))
    for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%d", "%Y-%m-%dT%H:%M:%S"):
        try:
            return datetime.strptime(date_str, fmt)
        except ValueError:
            continue
    return None

def get_all_anchors():
    anchors = []
    for root, dirs, files in os.walk(REPO_ROOT):
        if any(d in root for d in [".git", "target", ".cargo", ".agent"]):
            if ".agent/scripts" not in root: # Skip internal agent docs but scan scripts if needed
                continue
        for file in files:
            if not file.endswith((".rs", ".md", ".toml")):
                continue
            path = os.path.join(root, file)
            try:
                with open(path, "r", encoding="utf-8") as f:
                    lines = f.readlines()
                    current = None
                    for i, line in enumerate(lines):
                        if "ANCHOR:" in line:
                            match = ANCHOR_RE.search(line)
                            if match:
                                if current: anchors.append(current)
                                current = {"file": path, "line": i, "id": match.group(1), "meta": {}, "header_line": i}

                        if current and "//" in line:
                            parts = KV_RE.split(line)
                            if len(parts) > 1:
                                for j in range(1, len(parts), 2):
                                    key = parts[j]
                                    val = parts[j+1].split("//")[0].strip()
                                    for stop in ["WP", "PRIO", "NEEDS", "AGENT", "DATE", "CREATED", "STATUS", "DEADLINE", "SUCCESSOR", "DEPS"]:
                                        if f" {stop}:" in val:
                                            val = val.split(f" {stop}:")[0].strip()
                                    current["meta"][key] = val
                    if current: anchors.append(current)
            except: pass
    return anchors

def apply_changes(file_changes):
    for filepath, changes in file_changes.items():
        with open(filepath, "r") as f:
            lines = f.readlines()

        # Sort changes by line descending to keep indices valid
        sorted_changes = sorted(changes, key=lambda x: x["line"], reverse=True)
        for c in sorted_changes:
            line_idx = c["line"]
            if c["type"] == "update_status":
                # Scan a window starting from the header line
                for i in range(line_idx, min(line_idx + 10, len(lines))):
                    if "STATUS:" in lines[i]:
                        lines[i] = re.sub(r"STATUS:\s*[\w\x80-\xff→\s.+-]+", f"STATUS:{c['new_status']} ", lines[i])
                        break
                if c.get("comment"):
                     lines.insert(line_idx, c["comment"] + "\n")
            elif c["type"] == "clear_deps":
                for i in range(line_idx, min(line_idx + 10, len(lines))):
                    if "NEEDS:" in lines[i]:
                        lines[i] = re.sub(r"NEEDS:[\w:.-|]+", "NEEDS:NONE ", lines[i])
                    if "DEPS:" in lines[i]:
                        lines[i] = re.sub(r"DEPS:[\w:.-|]+", "DEPS:NONE ", lines[i])

        with open(filepath, "w") as f:
            f.writelines(lines)

def phase1_stale_wip(anchors, now, file_changes):
    print("Phase 1: Checking for stale WIP anchors...")
    for a in anchors:
        status = a["meta"].get("STATUS", "").upper()
        if any(s in status for s in ["WIP", "ACTIVE"]):
            date_str = a["meta"].get("DATE") or a["meta"].get("CREATED")
            dt = parse_date(date_str)
            if not dt: continue

            # If date only, assume end of day to be conservative
            if dt.hour == 0 and dt.minute == 0:
                dt = dt + timedelta(hours=23, minutes=59)

            if now - dt > STALE_THRESHOLD:
                print(f"WATCHDOG: Resetting stale {status} anchor {a['id']} in {a['file']}")
                if a["file"] not in file_changes: file_changes[a["file"]] = []
                file_changes[a["file"]].append({
                    "line": a["header_line"],
                    "type": "update_status",
                    "new_status": "OPEN",
                    "comment": "// WATCHDOG: Reset WIP due to timeout."
                })

def phase2_deadlocks(anchors, file_changes):
    print("Phase 2: Checking for deadlocks...")
    graph = {}
    blocked_anchors = {}
    for a in anchors:
        status = a["meta"].get("STATUS", "").upper()
        if "BLOCKED" in status:
            needs = a["meta"].get("NEEDS") or a["meta"].get("DEPS")
            if needs and needs != "NONE":
                deps = [d.strip() for d in re.split(r'[| ,]+', needs)]
                graph[a["id"]] = deps
                blocked_anchors[a["id"]] = a

    def find_cycle(node, visited, stack, path):
        visited.add(node)
        stack.add(node)
        path.append(node)
        for neighbor in graph.get(node, []):
            if neighbor not in visited:
                cycle = find_cycle(neighbor, visited, stack, path)
                if cycle: return cycle
            elif neighbor in stack:
                return path[path.index(neighbor):]
        stack.remove(node)
        path.pop()
        return None

    visited = set()
    for node in list(graph.keys()):
        if node not in visited:
            cycle = find_cycle(node, visited, set(), [])
            if cycle:
                print(f"WATCHDOG: Cycle detected: {' -> '.join(cycle)} -> {cycle[0]}")
                for break_node in cycle:
                    if break_node in blocked_anchors:
                        a = blocked_anchors[break_node]
                        print(f"WATCHDOG: Breaking cycle at {break_node}")
                        if a["file"] not in file_changes: file_changes[a["file"]] = []
                        file_changes[a["file"]].append({
                            "line": a["header_line"],
                            "type": "update_status",
                            "new_status": "OPEN",
                            "comment": "// WATCHDOG: Broken cyclic dependency."
                        })
                        file_changes[a["file"]].append({
                            "line": a["header_line"],
                            "type": "clear_deps"
                        })
                        break
                break

def phase3_fv_gates(now):
    print("Phase 3: Auditing Formal Verification Gates...")
    critical_files = [
        "crates/memfuse-store/src/lsm.rs",
        "crates/memfuse-store/src/wal.rs",
        "crates/memfuse-store/src/sstable.rs",
        "crates/memfuse-core/src/tx_buffer.rs",
        "crates/memfuse-core/src/types.rs"
    ]

    violations = []
    # Use git to find actual modified files by specific agents
    try:
        raw_diff = subprocess.check_output(
            ["git", "log", "--since='24 hours ago'", "--pretty=format:%H %an", "--", *critical_files],
            stderr=subprocess.DEVNULL
        ).decode()
        # This is a heuristic, in a real environment we would check Jules agent IDs in git config
    except:
        pass

    for f in critical_files:
        path = os.path.join(REPO_ROOT, f)
        if os.path.exists(path):
            with open(path, "r") as file:
                content = file.read()
                # Check for signatures of Jules-02 and Jules-10
                if ("AGENT:02" in content or "AGENT:10" in content) or ("@JULES-02" in content or "@JULES-10" in content):
                    has_verification = any(term in content for term in ["kani::proof", "#[kani::proof]", "TLA+", ".tla"])
                    if not has_verification:
                        violations.append(f)

    health_path = os.path.join(REPO_ROOT, "docs/HEALTH.md")
    gate_status = "OPEN" if violations else "DONE"

    print(f"WATCHDOG: Gate status is {gate_status}")

    content = [
        "# System Health & Verification Gates\n\n",
        "// ANCHOR:ARCH:GATE-FV — Formal Verification Gate\n",
        "// WP:WP-0.0 PRIO:1 NEEDS:NONE\n",
        f"// AGENT:00 DATE:{now.strftime('%Y-%m-%d')} STATUS:{gate_status}\n",
        "// WATCHDOG: Monitoring Kani/TLA+ proofs for LSM and Crypto components.\n"
    ]
    if violations:
        content.append(f"\n## FV Violations (Missing Kani/TLA+)\n")
        for v in violations:
            content.append(f"- {v}\n")

    os.makedirs(os.path.dirname(health_path), exist_ok=True)
    with open(health_path, "w") as f:
        f.writelines(content)

def phase4_pr_integration():
    print("Phase 4: GitHub PR Integration...")
    try:
        # Check if gh CLI is available
        subprocess.check_call(["which", "gh"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        # Get PRs with label 'jules'
        prs_raw = subprocess.check_output(
            ["gh", "pr", "list", "--label", "jules", "--json", "number,statusCheckRollup,mergeable"],
            text=True
        )
        prs = json.loads(prs_raw)

        for pr in prs:
            num = pr["number"]
            mergeable = pr["mergeable"] == "MERGEABLE"
            checks = pr.get("statusCheckRollup", [])

            # Filter for successful checks
            failures = [c for c in checks if c.get("conclusion") in ["FAILURE", "CANCELLED", "TIMED_OUT"]]
            pending = [c for c in checks if c.get("status") in ["IN_PROGRESS", "QUEUED", "WAITING"]]

            if mergeable and not failures and not pending and checks:
                print(f"WATCHDOG: PR #{num} passed Gates. Integrating...")
                integrate_script = os.path.join(REPO_ROOT, ".agent/scripts/jules-integrate.sh")
                if os.path.exists(integrate_script):
                    subprocess.run(["bash", integrate_script])
                else:
                    subprocess.run(["gh", "pr", "merge", str(num), "--merge", "--auto", "--delete-branch"])
    except:
        print("WATCHDOG: GitHub CLI not available or error in PR fetch. Skipping Phase 4.")

if __name__ == "__main__":
    now = get_now()
    anchors = get_all_anchors()
    file_changes = {}

    phase1_stale_wip(anchors, now, file_changes)
    phase2_deadlocks(anchors, file_changes)

    if file_changes:
        apply_changes(file_changes)

    phase3_fv_gates(now)
    phase4_pr_integration()
    print("--- Jules Watchdog Finished ---")
