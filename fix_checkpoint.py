import sys
path = 'crates/memfuse-store/src/checkpoint.rs'
with open(path, 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if 'assert_eq!(storage.get(' in line and '.unwrap()' in line:
        indent = line[:line.find('assert_eq!')]
        key = line[line.find('(')+1:line.find(').await')]
        expected = line[line.find('unwrap(), ')+10:line.find('));')]
        new_lines.append(f'{indent}let val = storage.get({key}).await.unwrap(); // unwrap allowed\n')
        new_lines.append(f'{indent}assert_eq!(val, {expected});\n')
    elif '// unwrap allowed' in line and 'let val' not in line:
        continue
    else:
        new_lines.append(line)

with open(path, 'w') as f:
    f.writelines(new_lines)
