import os
import re
from datetime import datetime, timedelta

ANCHOR_REGEX = re.compile(r"//\s*ANCHOR:(\S+)\s+STATUS:(\S+)(?:\s+AGENT:(\S+))?(?:\s+DATE:(\S+))?")
WIP_REGEX = re.compile(r"STATUS:WIP")
BLOCKED_REGEX = re.compile(r"STATUS:BLOCKED")

def scan_anchors():
    found_wip = []
    found_blocked = []

    for root, _, files in os.walk("."):
        if any(d in root for d in [".git", "target", "node_modules"]):
            continue

        for file in files:
            if not file.endswith((".rs", ".md")):
                continue

            path = os.path.join(root, file)
            with open(path, "r", errors="ignore") as f:
                lines = f.readlines()

            for i, line in enumerate(lines):
                if "STATUS:WIP" in line:
                    found_wip.append((path, i, line))
                if "STATUS:BLOCKED" in line:
                    found_blocked.append((path, i, line))

    return found_wip, found_blocked

def main():
    print("Watchdog Scan Start")
    wip, blocked = scan_anchors()

    # WIP Reset Logic (Placeholder for actual date parsing if available)
    for path, line_no, content in wip:
        print(f"FOUND WIP: {path}:{line_no+1} -> {content.strip()}")
        # Check date and reset if > 8h

    # Blocked Logic
    for path, line_no, content in blocked:
        print(f"FOUND BLOCKED: {path}:{line_no+1} -> {content.strip()}")

if __name__ == "__main__":
    main()
