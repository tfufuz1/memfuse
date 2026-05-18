import os
path = 'crates/memfuse-db/src/lib.rs'
with open(path, 'r') as f: lines = f.readlines()
new_lines = []
for line in lines:
    if 'pub distance_metric: memfuse_core::DistanceMetric,' in line:
        new_lines.append(line)
        new_lines.append('    pub encryption_passphrase: Option<String>,\n')
    elif 'distance_metric: memfuse_core::DistanceMetric::Cosine,' in line:
        new_lines.append(line)
        new_lines.append('            encryption_passphrase: None,\n')
    else:
        new_lines.append(line)
with open(path, 'w') as f: f.writelines(new_lines)
