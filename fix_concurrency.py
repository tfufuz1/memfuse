import sys

with open('crates/memfuse-checkpoint/tests/concurrency.rs', 'r') as f:
    lines = f.readlines()

with open('crates/memfuse-checkpoint/tests/concurrency.rs', 'w') as f:
    for line in lines:
        if '#[async_trait::async_trait]' in line:
            continue
        if 'impl StorageEngine for MockStorage {' in line:
            f.write('#[async_trait::async_trait]\n')
        f.write(line)
