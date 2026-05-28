import os
import re
import datetime
import sys

# Constants
TIMEOUT_HOURS = 8
CURRENT_DATE = datetime.datetime(2026, 5, 28, tzinfo=datetime.timezone.utc)

def get_now():
    return CURRENT_DATE

def parse_anchors(content):
    anchors = {}
    lines = content.splitlines()
    for i, line in enumerate(lines):
        if "ANCHOR:" in line:
            anchor_match = re.search(r"ANCHOR:(\S+)", line)
            if not anchor_match: continue
            anchor_id = anchor_match.group(1)

            # Find status, needs, created in this line OR adjacent lines (usually comments are blocks)
            # We look 3 lines up/down for associated info
            context = "\n".join(lines[max(0, i-1):min(len(lines), i+3)])

            status_match = re.search(r"STATUS:(\S+)", context)
            status = status_match.group(1) if status_match else "UNKNOWN"

            needs = []
            needs_match = re.search(r"NEEDS:(\S+)", context)
            if needs_match:
                needs = needs_match.group(1).split(",")

            deps_match = re.search(r"DEPS:(\S+)", context)
            if deps_match:
                needs.extend(deps_match.group(1).split(","))

            created_match = re.search(r"CREATED:(\d{4}-\d{2}-\d{2})", context)
            created = created_match.group(1) if created_match else None

            date_match = re.search(r"DATE:(\d{4}-\d{2}-\d{2})", context)
            if date_match and not created:
                created = date_match.group(1)

            agent_line = None
            for j in range(max(0, i-1), min(len(lines), i+3)):
                if "AGENT:" in lines[j]:
                    agent_line = lines[j]
                    break

            anchors[anchor_id] = {
                "status": status,
                "needs": [n for n in needs if n != "NONE"],
                "created": created,
                "line_no": i,
                "raw": line,
                "agent_line": agent_line
            }
    return anchors

def process_anchors():
    print("=== AGENT:00 Watchdog Start ===")

    all_anchors = {}
    file_contents = {}

    for root, dirs, files in os.walk("."):
        if any(d in root for d in [".git", "target", "docs/archive"]):
            continue

        for file in files:
            if not file.endswith((".rs", ".md", ".toml")):
                continue

            path = os.path.join(root, file)
            try:
                with open(path, 'r', encoding='utf-8') as f:
                    content = f.read()
                file_contents[path] = content
                anchors = parse_anchors(content)
                for aid, data in anchors.items():
                    data["path"] = path
                    all_anchors[aid] = data
            except Exception as e:
                print(f"Error reading {path}: {e}")

    # Phase 1: Stale WIP
    for aid, data in all_anchors.items():
        if data["status"] == "WIP" and data["created"]:
            try:
                created_date = datetime.datetime.strptime(data["created"], "%Y-%m-%d").replace(tzinfo=datetime.timezone.utc)
                if (get_now() - created_date).total_seconds() > TIMEOUT_HOURS * 3600:
                    print(f"Watchdog: Resetting stale WIP anchor {aid} in {data['path']}")
                    path = data['path']
                    content = file_contents[path]

                    old_line = data["raw"]
                    new_line = f"// WATCHDOG: Reset WIP due to timeout.\n{old_line.replace('STATUS:WIP', 'STATUS:OPEN')}"
                    file_contents[path] = content.replace(old_line, new_line)
            except ValueError:
                continue

    # Phase 2: Circular Deadlocks
    blocked = {aid: data for aid, data in all_anchors.items() if data["status"] == "BLOCKED"}

    def find_cycle(start_aid, current_aid, visited, path):
        if current_aid in path:
            cycle_start_idx = path.index(current_aid)
            return path[cycle_start_idx:]

        if current_aid in visited:
            return None

        visited.add(current_aid)

        if current_aid in all_anchors:
            for dep in all_anchors[current_aid]["needs"]:
                res = find_cycle(start_aid, dep, visited, path + [current_aid])
                if res: return res
        return None

    global_visited = set()
    cycles = []
    for aid in blocked:
        if aid not in global_visited:
            cycle = find_cycle(aid, aid, global_visited, [])
            if cycle:
                cycles.append(cycle)

    for cycle in cycles:
        print(f"Watchdog: Detected circular deadlock: {' -> '.join(cycle)} -> {cycle[0]}")
        aid_to_fix = cycle[0]
        data = all_anchors[aid_to_fix]
        path = data["path"]
        content = file_contents[path]

        old_line = data["raw"]
        new_line = old_line.replace("STATUS:BLOCKED", "STATUS:OPEN")
        for dep in cycle[1:]:
            if dep in new_line:
                 new_line = re.sub(rf",?{dep},?", "", new_line)

        if "NEEDS:" in new_line and new_line.split("NEEDS:")[1].strip() == "":
             new_line = new_line.replace("NEEDS:", "NEEDS:NONE")

        final_line = f"// WATCHDOG: Broken cyclic dependency.\n{new_line}"
        file_contents[path] = content.replace(old_line, final_line)

    # Phase 3: ARCH:GATE-FV
    core_lib_path = "./crates/memfuse-core/src/lib.rs"
    if core_lib_path in file_contents:
        lib_content = file_contents[core_lib_path]

        needs_gate_open = False
        for aid, data in all_anchors.items():
            if data["status"] == "REVIEW" and any(agent in (data.get("agent_line") or "") for agent in ["AGENT:02", "AGENT:10"]):
                content = file_contents[data["path"]]
                if "KANI:" not in content and "TLA+:" not in content:
                    needs_gate_open = True
                    break

        if needs_gate_open:
            if "ANCHOR:ARCH:GATE-FV STATUS:OPEN" not in lib_content:
                print("Watchdog: Opening ARCH:GATE-FV due to missing proofs.")
                lib_content = lib_content.replace("ANCHOR:ARCH:GATE-FV STATUS:DONE", "ANCHOR:ARCH:GATE-FV STATUS:OPEN")
                if "// WATCHDOG: Blocking merges" not in lib_content:
                    lib_content = lib_content.replace("ANCHOR:ARCH:GATE-FV STATUS:OPEN",
                                                      "ANCHOR:ARCH:GATE-FV STATUS:OPEN\n// WATCHDOG: Blocking merges due to missing Kani/TLA+ proofs for REVIEW components (WAL/LSM).")
                file_contents[core_lib_path] = lib_content

    for path, content in file_contents.items():
        with open(path, 'w', encoding='utf-8') as f:
            f.write(content)

    print("=== AGENT:00 Watchdog Complete ===")

if __name__ == "__main__":
    process_anchors()
