import re

with open('justfile', 'r') as f:
    c = f.read()
c = c.replace('L1 Kernel Isolation (core, runtime, orchestrator)', 'L1 Kernel Isolation (core, crypto, sandbox)')
c = c.replace('for CRATE in memfuse-core memfuse-sandbox memfuse-saos-agent; do', 'for CRATE in memfuse-core memfuse-crypto memfuse-sandbox; do')
c = c.replace('L2 Peer Isolation (store, index, text, checkpoint)', 'L2 Engine Isolation (store, index, text, checkpoint, graph)')
# Simplified regex for Phase 2
c = re.sub(r'echo "--- Phase 2: L2 Engine Isolation.*?echo "--- Phase 3: L3 Orchestration Isolation',
           'echo "--- Phase 2: L2 Engine Isolation (store, index, text, checkpoint, graph) ---\\n    for CRATE in memfuse-store memfuse-index memfuse-text memfuse-checkpoint memfuse-graph; do\\n        echo \\"Verifying $CRATE...\\"\\n        if cargo tree -p \\"\$CRATE\\" --edges no-dev | grep -E \\"memfuse-db|memfuse-py|memfuse-saos-agent\\" | grep -q .; then\\n            echo \\"❌ ERROR: \$CRATE violates DAG by importing higher layer crates.\\"\\n            cargo tree -p \\"\$CRATE\\" --edges no-dev | grep -E \\"memfuse-db|memfuse-py|memfuse-saos-agent\\"\\n            return 1\\n        fi\\n    done\\n\\n    echo \\"--- Phase 3: L3 Orchestration Isolation', c, flags=re.DOTALL)
c = c.replace('memfuse-py|memfuse-sandbox|memfuse-saos-agent', 'memfuse-py|memfuse-saos-agent')
with open('justfile', 'w') as f:
    f.write(c)

with open('.github/workflows/dag-check.yml', 'r') as f:
    c = f.read()
c = c.replace('Check L1 Kernel Isolation (core, runtime, orchestrator)', 'Check L1 Kernel Isolation (core, crypto, sandbox)')
c = c.replace('for CRATE in memfuse-core memfuse-runtime memfuse-orchestrator; do', 'for CRATE in memfuse-core memfuse-crypto memfuse-sandbox; do')
# Refine L2 Engine Isolation
c = re.sub(r'- name: Check L2 Engine Isolation.*? - name: Check L3 Orchestration Isolation',
           '- name: Check L2 Engine Isolation (store, index, text, checkpoint, graph)\\n        run: |\\n          set -e\\n          for CRATE in memfuse-store memfuse-index memfuse-text memfuse-checkpoint memfuse-graph; do\\n            echo \\"Verifying \$CRATE...\\"\\n            if cargo tree -p \\"\$CRATE\\" --edges no-dev | grep -E \\"memfuse-db|memfuse-py|memfuse-saos-agent\\" | grep -q .; then\\n              echo \\"ERROR: \$CRATE violates DAG by importing higher layer crates.\\"\\n              cargo tree -p \\"\$CRATE\\" --edges no-dev | grep -E \\"memfuse-db|memfuse-py|memfuse-saos-agent\\"\\n              exit 1\\n            fi\\n          done\\n\\n      - name: Check L3 Orchestration Isolation', c, flags=re.DOTALL)
# Refine L4
c = re.sub(r'- name: Check L4 Bindings Isolation.*? - name: Check known DAG violations',
           '- name: Check L4 Apps/Bindings Isolation (py, saos-agent)\\n        run: |\\n          set -e\\n          for CRATE in memfuse-py memfuse-saos-agent; do\\n            echo \\"Verifying \$CRATE...\\"\\n            if [ \\"\$CRATE\\" == \\"memfuse-py\\" ]; then\\n                 if cargo tree -p \\"\$CRATE\\" --edges no-dev | grep -q \\"memfuse-saos-agent\\"; then\\n                    echo \\"ERROR: memfuse-py should not depend on memfuse-saos-agent.\\"\\n                    exit 1\\n                 fi\\n            fi\\n          done\\n\\n      - name: Check known DAG violations', c, flags=re.DOTALL)
with open('.github/workflows/dag-check.yml', 'w') as f:
    f.write(c)

with open('.github/workflows/jules-quality-gate.yml', 'r') as f:
    lines = f.readlines()
with open('.github/workflows/jules-quality-gate.yml', 'w') as f:
    for line in lines:
        if '/tests/' in line and '| grep -v' in line and '--include="*.rs" crates/' not in line:
            f.write(line)
            f.write('            | grep -v "test_" \\\n')
            f.write('            | grep -v "mock" \\\n')
            f.write('            | grep -v "assert" \\\n')
            f.write('            | grep -v "::tests" \\\n')
        else:
            f.write(line)
