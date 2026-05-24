path = 'crates/memfuse-store/src/compaction.rs'
with open(path, 'r') as f:
    lines = f.readlines()
with open(path, 'w') as f:
    for line in lines:
        if 'let stats = storage.stats().await.expect("stats");' in line:
            f.write('            let stats = storage.stats().await.expect("stats"); // unwrap allowed\n')
        elif 'compaction is doing its job' in line and 'let stats' not in line:
            # Preserve the comment if it was moved
            if '// If we have few segments' in line:
                 f.write('            // If we have few segments, compaction is doing its job\n')
            continue
        else:
            f.write(line)
