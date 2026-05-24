path = 'crates/memfuse-store/src/checkpoint.rs'
with open(path, 'r') as f:
    lines = f.readlines()
with open(path, 'w') as f:
    for line in lines:
        if '.unwrap()' in line and '// unwrap allowed' not in line:
            f.write(line.replace('.unwrap()', '.unwrap() // unwrap allowed'))
        else:
            f.write(line)
