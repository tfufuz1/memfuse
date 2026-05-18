import os
path = 'crates/memfuse-db/src/lib.rs'
with open(path, 'r') as f: content = f.read()

# Fix the test at the bottom
parts = content.split('#[cfg(test)]')
if len(parts) > 1:
    parts[-1] = parts[-1].replace('distance_metric: DistanceMetric::Cosine,', 'distance_metric: DistanceMetric::Cosine, encryption_passphrase: None,')
    content = '#[cfg(test)]'.join(parts)

with open(path, 'w') as f: f.write(content)
