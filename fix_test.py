import os

path = 'crates/memfuse-orchestrator/tests/e2e_integration.rs'
with open(path, 'r') as f:
    content = f.read()

# The error E0063 happens because of the new encryption_passphrase field.
# I will use search and replace to add the field.

content = content.replace(
    'distance_metric: DistanceMetric::Cosine,',
    'distance_metric: DistanceMetric::Cosine,\n        encryption_passphrase: None,'
)

with open(path, 'w') as f:
    f.write(content)
