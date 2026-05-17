import os
import re

files = [
    "./crates/memfuse-checkpoint/tests/layer_bounds.rs",
    "./crates/memfuse-db/tests/layer_bounds.rs",
    "./crates/memfuse-db/tests/atomic_commit.rs",
    "./crates/memfuse-db/tests/full_stack_e2e.rs",
    "./crates/memfuse-db/tests/concurrent_collection_stress.rs",
    "./crates/memfuse-db/tests/stress.rs",
    "./crates/memfuse-orchestrator/tests/e2e_integration.rs"
]

for filepath in files:
    if not os.path.exists(filepath): continue
    with open(filepath, 'r') as f:
        content = f.read()

    # regex to find MemFuseConfig { ... }
    # but be careful with nesting

    parts = re.split(r'(MemFuseConfig\s*\{)', content)
    new_parts = [parts[0]]
    for i in range(1, len(parts), 2):
        header = parts[i]
        body_rest = parts[i+1]

        # find matching brace
        brace_count = 1
        j = 0
        while j < len(body_rest) and brace_count > 0:
            if body_rest[j] == '{': brace_count += 1
            elif body_rest[j] == '}': brace_count -= 1
            j += 1

        body = body_rest[:j-1]
        rest = body_rest[j-1:]

        if '..Default::default()' not in body:
            # check if it needs a comma
            stripped = body.rstrip()
            if stripped and not stripped.endswith(','):
                body = stripped + ', ..Default::default()'
            else:
                body = stripped + ' ..Default::default()'

        new_parts.append(header + body + rest)

    new_content = "".join(new_parts)
    if new_content != content:
        with open(filepath, 'w') as f:
            f.write(new_content)
        print(f"Fixed {filepath}")
