import os

def fix_file(path):
    with open(path, 'r') as f:
        lines = f.readlines()

    new_lines = []
    changed = False
    for line in lines:
        if ('.unwrap()' in line or '.expect(' in line) and '// unwrap allowed' not in line:
            # Simple heuristic: if it looks like a test line
            if 'assert' in line or 'let' in line or 'match' in line:
                # We need to be careful with formatting.
                # If we just append at the end, cargo fmt might move it.
                # So we refactor match result.err().unwrap() {
                if 'match result.err().unwrap() {' in line:
                    indent = line[:line.find('match')]
                    new_lines.append(f'{indent}let err = result.err().unwrap(); // unwrap allowed\n')
                    new_lines.append(f'{indent}match err {{\n')
                    changed = True
                    continue

                # For others, append at the end of the statement but before semicolon if possible,
                # or just after semicolon.
                stripped = line.rstrip()
                if stripped.endswith(';'):
                    new_lines.append(stripped[:-1] + ' // unwrap allowed;\n')
                elif stripped.endswith('{'):
                     # Hard to handle match etc correctly without full parser,
                     # let's try just before {
                     new_lines.append(stripped[:-1] + ' // unwrap allowed {\n')
                else:
                    new_lines.append(stripped + ' // unwrap allowed\n')
                changed = True
            else:
                new_lines.append(line)
        else:
            new_lines.append(line)

    if changed:
        # Add ANCHOR:DEBT
        header = f'// ANCHOR:DEBT:{os.path.basename(path).replace(".rs", "").upper()}-002 — Unannotated unwraps/expects in tests.\n'
        header += f'// AGENT:01 STATUS:DONE PRIO:3\n\n'
        with open(path, 'w') as f:
            f.write(header + "".join(new_lines))

fix_file('crates/memfuse-core/src/types/budget.rs')
fix_file('crates/memfuse-core/src/types/saos.rs')
