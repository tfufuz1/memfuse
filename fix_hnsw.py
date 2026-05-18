import os
path = 'crates/memfuse-index/src/hnsw.rs'
with open(path, 'r') as f: lines = f.readlines()
new_lines = []
for line in lines:
    if 'struct HnswNode {' in line:
        new_lines.append('#[derive(Debug, Clone)]\n')
        new_lines.append('pub(crate) struct HnswNode {\n')
    elif '    doc_id: DocId,' in line: new_lines.append('    pub(crate) doc_id: DocId,\n')
    elif '    vector: VectorData,' in line: new_lines.append('    pub(crate) vector: VectorData,\n')
    elif '    connections: Vec<Vec<usize>>,' in line: new_lines.append('    pub(crate) connections: Vec<Vec<usize>>,\n')
    elif '    _max_layer: usize,' in line: new_lines.append('    pub(crate) _max_layer: usize,\n')
    elif '    config: HnswConfig,' in line and 'HnswIndexCore' in "".join(new_lines[-10:]):
        new_lines.append('    pub(crate) config: HnswConfig,\n')
    elif '    // Kept for backward compatibility' in line:
        new_lines.append('    pub(crate) fn get_nodes_for_diskann(&self) -> Vec<HnswNode> { self.nodes.read().clone() }\n')
        new_lines.append('    pub(crate) fn get_entry_point_for_diskann(&self) -> Option<usize> { *self.entry_point.read() }\n')
        new_lines.append(line)
    elif 'node.vector = VectorData::U8(q.quantize(v));' in line:
        new_lines.append(line.replace('q.quantize(v)', 'q.quantize(&v)'))
    else:
        new_lines.append(line)
with open(path, 'w') as f: f.writelines(new_lines)
