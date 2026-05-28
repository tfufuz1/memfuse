import os

with open('crates/memfuse-index/src/hnsw.rs', 'r') as f:
    content = f.read()

# 1. Ensure HnswNode derives Clone
content = content.replace('struct HnswNode {', '#[derive(Clone)]\nstruct HnswNode {')

# 2. Refactor save()
search_text = """        let _lock = self.write_mutex.lock().await;
        // ANCHOR:FIX:D2-004 — Holding guards across await points (WP-0.0)
        // AGENT:13 DATE:2026-05-28 STATUS:DONE
        let (nodes_snap, ep_snap, q_snap) = {
            let n = self.nodes.read();
            let ep = self.entry_point.read();
            let q = self.quantizer.read();
            (n.clone(), *ep, q.clone())
        };
        let nodes = &nodes_snap;
        let entry_point = ep_snap;
        let q_guard = q_snap;"""

# I need to use the actual types to help the compiler
replace_text = """        let _lock = self.write_mutex.lock().await;
        // ANCHOR:FIX:D2-004 — Holding guards across await points (WP-0.0)
        // AGENT:13 DATE:2026-05-28 STATUS:DONE
        let (nodes_snap, ep_snap, q_snap): (Vec<HnswNode>, Option<usize>, Option<crate::distance::ScalarQuantizer>) = {
            let n = self.nodes.read();
            let ep = self.entry_point.read();
            let q = self.quantizer.read();
            (n.clone(), *ep, q.clone())
        };
        let nodes = &nodes_snap;
        let entry_point = ep_snap;
        let q_guard = &q_snap;"""

new_content = content.replace(search_text, replace_text)

# 3. Fix the write_all(v) error
new_content = new_content.replace('.write_all(v)', '.write_all(&v)')

with open('crates/memfuse-index/src/hnsw.rs', 'w') as f:
    f.write(new_content)
