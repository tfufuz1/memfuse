import os
import re
from datetime import datetime, timedelta

# Constants
STALE_TIMEOUT_HOURS = 8
STATUS_WIP = "STATUS:WIP"
STATUS_OPEN = "STATUS:OPEN"
STATUS_BLOCKED = "STATUS:BLOCKED"
NOW = datetime(2026, 5, 20, 0, 30)  # Simulated NOW from task context

def reset_stale_anchors():
    print("--- Phase 1: Resetting Stale Anchors ---")
    for root, _, files in os.walk("crates"):
        for file in files:
            if not file.endswith(".rs"): continue
            path = os.path.join(root, file)
            with open(path, "r") as f:
                content = f.read()

            if STATUS_WIP not in content: continue

            # Simple regex to find WIP anchors and their dates
            # Expecting // ANCHOR:... STATUS:WIP ... DATE:YYYY-MM-DD
            # Or WIP-START:YYYY-MM-DD
            updated = False
            lines = content.splitlines()
            for i, line in enumerate(lines):
                if STATUS_WIP in line:
                    date_match = re.search(r"DATE:(\d{4}-\d{2}-\d{2})", line)
                    if date_match:
                        date_str = date_match.group(1)
                        try:
                            anchor_date = datetime.strptime(date_str, "%Y-%m-%d")
                            if NOW - anchor_date > timedelta(hours=STALE_TIMEOUT_HOURS):
                                print(f"Resetting stale anchor in {path}")
                                lines[i] = line.replace(STATUS_WIP, STATUS_OPEN)
                                lines.insert(i, "// WATCHDOG: Reset WIP due to timeout.")
                                updated = True
                        except ValueError:
                            pass

            if updated:
                with open(path, "w") as f:
                    f.write("\n".join(lines) + "\n")

def solve_deadlocks():
    print("--- Phase 2: Solving Deadlocks ---")
    # Deadlock detection logic (placeholder/simplified)
    # Search for STATUS:BLOCKED and DEPS:
    pass

def monitor_fv_gate():
    print("--- Phase 3: Monitoring FV Gate ---")
    gate_file = "crates/memfuse-core/src/lib.rs"
    if not os.path.exists(gate_file): return

    with open(gate_file, "r") as f:
        content = f.read()

    # Check if REVIEW components exist
    review_components = []
    for root, _, files in os.walk("crates"):
        for file in files:
            if not file.endswith(".rs"): continue
            with open(os.path.join(root, file), "r") as f:
                if "STATUS:REVIEW" in f.read():
                    review_components.append(file)

    if review_components:
        if "ARCH:GATE-FV STATUS:OPEN" not in content:
            print("Opening FV Gate due to components in REVIEW")
            content = content.replace("ARCH:GATE-FV STATUS:DONE", "ARCH:GATE-FV STATUS:OPEN")
            with open(gate_file, "w") as f:
                f.write(content)
    else:
        print("All components reviewed, FV Gate can be CLOSED (if logic allowed)")

if __name__ == "__main__":
    reset_stale_anchors()
    solve_deadlocks()
    monitor_fv_gate()
