import os
import re

def fix_file(filepath):
    if not os.path.exists(filepath):
        return
    with open(filepath, 'r') as f:
        content = f.read()

    fields = ['dimension', 'max_elements', 'm', 'ef_construction', 'ef_search',
              'distance_metric', 'rebuild_threshold', 'quantize']

    pattern = re.compile(r'HnswConfig\s*\{([^\}]*)\}', re.MULTILINE)

    def replace_config(match):
        inner = match.group(1)
        if '..Default::default()' not in inner:
            return match.group(0)

        present_fields = [f for f in fields if f + ':' in inner]
        if len(present_fields) == len(fields):
            new_inner = inner.replace('..Default::default()', '').strip()
            if new_inner.endswith(','):
                new_inner = new_inner[:-1].strip()
            return "HnswConfig {\n        " + new_inner + "\n    }"
        return match.group(0)

    new_content = pattern.sub(replace_config, content)

    if new_content != content:
        with open(filepath, 'w') as f:
            f.write(new_content)

fix_file('crates/memfuse-index/tests/recall.rs')
fix_file('crates/memfuse-index/tests/ram_reduction.rs')
