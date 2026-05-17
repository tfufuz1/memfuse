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

    # We want to insert ..Default::default() before the closing } of MemFuseConfig { ... }
    # but only if it's not already there.

    pattern = re.compile(r'(MemFuseConfig\s*\{[^}]*?)\s*\}', re.DOTALL)

    def repl(m):
        inner = m.group(1)
        if '..Default::default()' in inner:
            return m.group(0)

        # Add comma if needed
        stripped = inner.rstrip()
        if not stripped.endswith(',') and not stripped.endswith('{'):
            return stripped + ',\n        ..Default::default()\n    }'
        else:
            return stripped + '\n        ..Default::default()\n    }'

    new_content = pattern.sub(repl, content)

    if new_content != content:
        with open(filepath, 'w') as f:
            f.write(new_content)
        print(f"Fixed {filepath}")
