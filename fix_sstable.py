import re
with open('crates/memfuse-store/src/sstable.rs', 'r') as f:
    content = f.read()
# Avoid parentheses disaster
content = content.replace('data[0..8].try_into().unwrap()', 'data[0..8].try_into().expect("fixed size buffer")')
content = content.replace('data[8..16].try_into().unwrap()', 'data[8..16].try_into().expect("fixed size buffer")')
content = content.replace('data[offset..offset + 8].try_into().unwrap()', 'data[offset..offset + 8].try_into().expect("fixed size buffer")')
content = content.replace('data[file_size - 12..file_size - 4].try_into().unwrap()', 'data[file_size - 12..file_size - 4].try_into().expect("fixed size buffer")')
content = content.replace('.try_into().unwrap()', '.try_into().expect("fixed size buffer")')
with open('crates/memfuse-store/src/sstable.rs', 'w') as f:
    f.write(content)
