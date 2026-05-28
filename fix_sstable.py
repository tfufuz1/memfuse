import os

with open('crates/memfuse-store/src/sstable.rs', 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    # BloomFilter::from_bytes
    if 'data[0..8].try_into().unwrap()' in line:
        line = line.replace('data[0..8].try_into().unwrap()', 'data[0..8].try_into().map_err(|_| MemFuseError::Storage("invalid bloom filter hashes".into()))?')
    if 'data[8..16].try_into().unwrap()' in line:
        line = line.replace('data[8..16].try_into().unwrap()', 'data[8..16].try_into().map_err(|_| MemFuseError::Storage("invalid bloom filter bits".into()))?')
    if 'data[offset..offset + 8].try_into().unwrap()' in line:
        line = line.replace('data[offset..offset + 8].try_into().unwrap()', 'data[offset..offset + 8].try_into().map_err(|_| MemFuseError::Storage("invalid bloom filter word".into()))?')

    # SstableReader::open_with_key_manager
    if '.try_into().unwrap()' in line and 'mmap.get(' in line:
        line = line.replace('.try_into().unwrap()', '.try_into().map_err(|_| MemFuseError::Storage("invalid trailer slice".into()))?')

    # Test unwrap
    if 'data[file_size - 12..file_size - 4].try_into().unwrap()' in line:
        line = line.replace('unwrap()', 'unwrap()); // unwrap allowed (AGENT:02)')

    new_lines.append(line)

with open('crates/memfuse-store/src/sstable.rs', 'w') as f:
    f.writelines(new_lines)
