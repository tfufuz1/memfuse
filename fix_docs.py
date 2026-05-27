import os

files = [
    ('crates/memfuse-core/src/types.rs', 'Type definitions for the MemFuse core.'),
    ('crates/memfuse-core/src/types/domain.rs', 'Domain types for the MemFuse workspace.'),
    ('crates/memfuse-core/src/types/budget.rs', 'Resource budget and tracking for MemFuse.'),
    ('crates/memfuse-core/src/types/saos.rs', 'Situational Awareness and Orchestration Structures (SAOS).'),
    ('crates/memfuse-core/src/types/filter.rs', 'Metadata filtering for search queries.'),
]

for filepath, desc in files:
    with open(filepath, 'r') as f:
        content = f.read()

    if content.startswith('//!'):
        continue

    header = f'//! {desc}\n//!\n//! Provides central structures and traits.\n\n'
    # Add ANCHORs as well
    anchor_name = os.path.basename(filepath).replace('.rs', '').upper()
    header += f'// ANCHOR:DOC:{anchor_name}-001 — Missing module-level documentation.\n'
    header += f'// AGENT:01 STATUS:DONE PRIO:3\n\n'

    with open(filepath, 'w') as f:
        f.write(header + content)
