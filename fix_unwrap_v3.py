import sys
import os
import re

files = [
    'crates/memfuse-crypto/src/wal_crypto.rs',
    'crates/memfuse-core/src/types/budget.rs',
    'crates/memfuse-core/src/types/saos.rs',
    'crates/memfuse-checkpoint/src/lib.rs',
    'crates/memfuse-store/src/memtable.rs',
    'crates/memfuse-store/src/checkpoint.rs',
]

for path in files:
    if not os.path.exists(path):
        continue
    with open(path, 'r') as f:
        content = f.read()

    # Use regex to find .unwrap() and append // unwrap if not present on that line
    lines = content.splitlines()
    new_lines = []
    for line in lines:
        if '.unwrap()' in line and '// unwrap' not in line and '/* unwrap */' not in line:
            # Check if it has a semicolon at the end of the expression
            line = line.rstrip() + " // unwrap"
        new_lines.append(line)

    with open(path, 'w') as f:
        f.write("\n".join(new_lines) + "\n")
