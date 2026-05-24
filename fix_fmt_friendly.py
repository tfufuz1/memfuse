path = 'crates/memfuse-core/src/types/budget.rs'
with open(path, 'r') as f:
    content = f.read()

bad = 'match result.err().unwrap() { // unwrap allowed'
good = 'let err = result.err().unwrap(); // unwrap allowed\n        match err {'
new_content = content.replace(bad, good)
with open(path, 'w') as f:
    f.write(new_content)
