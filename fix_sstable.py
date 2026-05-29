import sys
import re

with open('crates/memfuse-store/src/sstable.rs', 'r') as f:
    content = f.read()

# Replace all try_into().unwrap() with try_into().expect(...) // unwrap allowed
# Use regex to avoid double matching
content = re.sub(r'(\.try_into\(\))\s*\.unwrap\(\)', r'\1.expect("fixed size slice") // unwrap allowed (AGENT:11)', content)

with open('crates/memfuse-store/src/sstable.rs', 'w') as f:
    f.write(content)
