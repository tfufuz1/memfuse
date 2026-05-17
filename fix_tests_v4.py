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

    # Identify MemFuseConfig blocks that do NOT have ..Default::default()
    # and add it.

    parts = re.split(r'(MemFuseConfig\s*\{)', content)
    new_parts = [parts[0]]
    for i in range(1, len(parts), 2):
        header = parts[i]
        body_rest = parts[i+1]

        # find the matching closing brace for this block
        brace_count = 1
        j = 0
        while j < len(body_rest) and brace_count > 0:
            if body_rest[j] == '{': brace_count += 1
            elif body_rest[j] == '}': brace_count -= 1
            j += 1

        block_body = body_rest[:j-1]
        remainder = body_rest[j-1:]

        if '..Default::default()' not in block_body:
            # Add it
            stripped = block_body.rstrip()
            if not stripped.endswith(',') and not stripped.endswith('{'):
                block_body = stripped + ',\n        ..Default::default()'
            else:
                block_body = stripped + '\n        ..Default::default()'

        new_parts.append(header)
        new_parts.append(block_body + remainder)

    new_content = "".join(new_parts)
    if new_content != content:
        with open(filepath, 'w') as f:
            f.write(new_content)
        print(f"Fixed {filepath}")
