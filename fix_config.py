import os
import re

def fix_file(path):
    with open(path, 'r') as f:
        lines = f.readlines()

    new_lines = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if "MemFuseConfig {" in line:
            j = i
            while j < len(lines) and "}" not in lines[j]:
                j += 1
            block_lines = lines[i:j+1]
            clean_block = []
            found_default = False
            for bl in block_lines:
                if "..Default::default()" in bl:
                    if not found_default:
                        clean_block.append("        ..Default::default()\n")
                        found_default = True
                else:
                    clean_block.append(bl)
            if not found_default:
                clean_block.insert(-1, "        ..Default::default()\n")
            new_lines.extend(clean_block)
            i = j + 1
        else:
            new_lines.append(line)
            i += 1

    with open(path, 'w') as f:
        f.writelines(new_lines)

files = [
    "crates/memfuse-db/tests/concurrent_collection_stress.rs",
    "crates/memfuse-db/tests/full_stack_e2e.rs",
    "crates/memfuse-orchestrator/tests/e2e_integration.rs",
    "crates/memfuse-db/tests/layer_bounds.rs",
    "crates/memfuse-db/tests/stress.rs",
    "crates/memfuse-db/tests/atomic_commit.rs",
    "crates/memfuse-checkpoint/tests/layer_bounds.rs",
    "crates/memfuse-py/src/lib.rs"
]

for p in files:
    if os.path.exists(p):
        fix_file(p)
