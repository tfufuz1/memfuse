path = 'crates/memfuse-core/src/types/budget.rs'
with open(path, 'r') as f:
    lines = f.readlines()
with open(path, 'w') as f:
    for line in lines:
        if 'match result.err().unwrap() {' in line:
            f.write('        let err = result.err().unwrap(); // unwrap allowed\n')
            f.write('        match err {\n')
        else:
            f.write(line)
