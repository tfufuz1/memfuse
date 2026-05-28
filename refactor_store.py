import os

def refactor_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # Refactor assert_eq!(storage.get(...).await.unwrap(), ...)
    import re

    # Simple regex for some common patterns
    content = re.sub(r'assert_eq!\((storage\.get\(.*?\)\.await\.unwrap\(\)), (.*?)\); // unwrap allowed \(AGENT:02\)',
                     r'let val = \1; // unwrap allowed (AGENT:02)\n        assert_eq!(val, \2);', content)

    with open(filepath, 'w') as f:
        f.write(content)

refactor_file('crates/memfuse-store/src/checkpoint.rs')
refactor_file('crates/memfuse-store/src/lsm.rs')
