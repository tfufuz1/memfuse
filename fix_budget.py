import sys
path = 'crates/memfuse-core/src/types/budget.rs'
with open(path, 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if 'match result.err().unwrap()' in line:
        indent = line[:line.find('match')]
        new_lines.append(f'{indent}let err = result.err().unwrap(); // unwrap allowed\n')
        new_lines.append(f'{indent}match err {{\n')
    elif '// unwrap allowed' in line and 'match' not in line and 'let err' not in line:
        continue # Skip the orphaned comments
    else:
        new_lines.append(line)

with open(path, 'w') as f:
    f.writelines(new_lines)
