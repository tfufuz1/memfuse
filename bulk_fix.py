import os, re
def fix(p):
    if not os.path.exists(p): return
    if 'crates/memfuse-db/src/lib.rs' in p: return # Handled already
    with open(p, 'r') as f: c = f.read()
    def repl(m):
        inner = m.group(1)
        if '..Default::default()' in inner: return m.group(0)
        return f"MemFuseConfig {{ {inner.strip().rstrip(',')}, ..Default::default() }}"
    c = re.sub(r'MemFuseConfig\s*\{([^}]*)\}', repl, c, flags=re.DOTALL)
    with open(p, 'w') as f: f.write(c)
for root, _, files in os.walk('.'):
    if 'target' in root: continue
    for f in files:
        if f.endswith('.rs'): fix(os.path.join(root, f))
