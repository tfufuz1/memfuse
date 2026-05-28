import os

with open('crates/memfuse-index/src/hnsw.rs', 'r') as f:
    content = f.read()

search_text = """        let _lock = self.write_mutex.lock().await;
        let nodes = self.nodes.read();
        let entry_point = self.entry_point.read();
        let q_guard = self.quantizer.read();"""

replace_text = """        let _lock = self.write_mutex.lock().await;
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

new_content = content.replace(search_text, replace_text)

with open('crates/memfuse-index/src/hnsw.rs', 'w') as f:
    f.write(new_content)
