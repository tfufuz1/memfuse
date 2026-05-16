import re
import sys
import os

def check_file(filepath):
    if not os.path.exists(filepath):
        return []
    with open(filepath, 'r') as f:
        content = f.read()

    unsafe_matches = list(re.finditer(r'(?m)^.*(?:unsafe\s+\{|unsafe\s+fn|pub\s+unsafe\s+fn).*$', content))

    issues = []
    lines = content.splitlines()

    for match in unsafe_matches:
        line_num = content.count('\n', 0, match.start()) + 1
        line_content = match.group(0).strip()

        if line_content.startswith('//') or line_content.startswith('*'):
            continue

        found_safety = False
        found_begrundung = False

        for j in range(line_num - 2, max(-1, line_num - 15), -1):
            prev_line = lines[j].strip()
            if 'ANCHOR:SAFETY' in prev_line:
                found_safety = True
            if 'BEGRÜNDUNG' in prev_line:
                found_begrundung = True

        if not (found_safety and found_begrundung):
            issues.append((line_num, line_content))

    return issues

if __name__ == "__main__":
    target = 'crates/memfuse-index/src/distance.rs'
    issues = check_file(target)
    if issues:
        print(f"Found {len(issues)} undocumented unsafe occurrences in {target}:")
        for line_num, content in issues:
            print(f"  Line {line_num}: {content}")
        sys.exit(1)
    else:
        print(f"All unsafe occurrences in {target} are properly documented.")
        sys.exit(0)
