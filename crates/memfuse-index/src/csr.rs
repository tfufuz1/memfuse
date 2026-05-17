//! Compressed Sparse Row (CSR) Graph for relation tracking.

// ANCHOR:ARCH:CSR-001 — Compressed Sparse Row (CSR) Graph für Memory-optimierte Relationen.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// ZIEL: Extrem schnelle, cache-lokale Multi-Hop-Traversierung für SAOS Agents.
// DESIGN: Static Graph (wird via `build_from_dynamic` ge-freezed).
// VERWENDET FÜR: Agent-Interaction-Graphs, Task-Hierarchien.

use std::collections::HashMap;

/// A simple CSR graph representation for fast neighborhood lookups.
/// This enables extremely fast multi-hop reasoning by storing edges contiguously.
#[derive(Debug, Default)]
pub struct CsrGraph {
    /// Array of offsets mapping node index -> start of its edges in `edges`
    pub offsets: Vec<usize>,
    /// Contiguous array of destination node indices
    pub edges: Vec<u32>,
    /// Relation types for each edge, mapping 1:1 with `edges`
    pub relation_types: Vec<u8>,

    /// Map string ID to internal u32 index
    pub node_map: HashMap<String, u32>,
    /// Map internal u32 index back to string ID
    pub rev_map: Vec<String>,
}

impl CsrGraph {
    /// Creates a new, empty CsrGraph.
    pub fn new() -> Self {
        Self {
            offsets: vec![0],
            edges: Vec::new(),
            relation_types: Vec::new(),
            node_map: HashMap::new(),
            rev_map: Vec::new(),
        }
    }

    /// Gets or creates an internal node ID
    pub fn get_or_create_node(&mut self, id: &str) -> u32 {
        if let Some(&idx) = self.node_map.get(id) {
            idx
        } else {
            let idx = self.rev_map.len() as u32;
            self.node_map.insert(id.to_string(), idx);
            self.rev_map.push(id.to_string());
            idx
        }
    }

    /// Freezes a dynamic edge list into the static CSR representation
    pub fn build_from_dynamic(&mut self, adj_list: &HashMap<u32, Vec<(u32, u8)>>) {
        let num_nodes = self.rev_map.len();
        self.offsets = Vec::with_capacity(num_nodes + 1);
        self.edges = Vec::new();
        self.relation_types = Vec::new();

        let mut current_offset = 0;
        for i in 0..num_nodes {
            self.offsets.push(current_offset);
            if let Some(neighbors) = adj_list.get(&(i as u32)) {
                for &(target, rel) in neighbors {
                    self.edges.push(target);
                    self.relation_types.push(rel);
                    current_offset += 1;
                }
            }
        }
        self.offsets.push(current_offset);
    }

    /// Returns the outgoing edges for a given node ID
    pub fn get_neighbors(&self, id: &str) -> Vec<(&str, u8)> {
        let mut result = Vec::new();
        if let Some(&idx) = self.node_map.get(id) {
            let idx = idx as usize;
            if idx < self.offsets.len() - 1 {
                let start = self.offsets[idx];
                let end = self.offsets[idx + 1];
                for i in start..end {
                    let target_idx = self.edges[i] as usize;
                    let target_id = &self.rev_map[target_idx];
                    let rel = self.relation_types[i];
                    result.push((target_id.as_str(), rel));
                }
            }
        }
        result
    }
}
