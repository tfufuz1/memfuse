import os

with open('crates/memfuse-sandbox/src/host_functions.rs', 'r') as f:
    content = f.read()

# Fix memory_growing
content = content.replace('current: u64,', 'current: usize,')
content = content.replace('desired: u64,', 'desired: usize,')
content = content.replace('maximum: Option<u64>,', 'maximum: Option<usize>,')

# Fix table_growing
content = content.replace('current: u32,', 'current: usize,')
content = content.replace('desired: u32,', 'desired: usize,')
content = content.replace('maximum: Option<u32>,', 'maximum: Option<usize>,')

with open('crates/memfuse-sandbox/src/host_functions.rs', 'w') as f:
    f.write(content)
