import os

def fix_file(path, pattern, replacement):
    with open(path, 'r') as f:
        content = f.read()
    new_content = content.replace(pattern, replacement)
    with open(path, 'w') as f:
        f.write(new_content)

# budget.rs
fix_file('crates/memfuse-core/src/types/budget.rs',
         'match result.err().unwrap() {\n            // unwrap allowed',
         'match result.err().unwrap() { // unwrap allowed')

# checkpoint.rs
# Find the line and make sure unwrap allowed is at the end
with open('crates/memfuse-store/src/checkpoint.rs', 'r') as f:
    lines = f.readlines()
with open('crates/memfuse-store/src/checkpoint.rs', 'w') as f:
    for line in lines:
        if 'assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec()));' in line:
            f.write('        assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec())); // unwrap allowed\n')
        else:
            f.write(line)
