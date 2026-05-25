import sys

with open('crates/memfuse-index/src/hnsw.rs', 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    # Fix write_all/flush on BufWriter
    if 'writer.write_all(' in line:
        line = line.replace('writer.write_all(', 'Write::write_all(&mut writer, ')
    if 'writer.flush()' in line:
        line = line.replace('writer.flush()', 'Write::flush(&mut writer)')

    # Fix write_all on File (Final Updates section)
    if 'file.write_all(' in line:
        line = line.replace('file.write_all(', 'Write::write_all(&mut file, ')

    # Fix u32 indexing in select_neighbors_heuristic calls
    # Handled via replace_with_git_merge_diff earlier for some cases

    # Fix new_idx type in conn_layer.push(new_idx)
    if 'conn_layer.push(new_idx);' in line:
        line = line.replace('conn_layer.push(new_idx);', 'conn_layer.push(new_idx as u32);')

    # Fix Candidate index type in search_layer and elsewhere if needed
    # (Checking against previous cargo check output)

    if 'index: idx,' in line and 'Candidate {' in lines[lines.index(line)-1]:
         line = line.replace('index: idx,', 'index: idx as usize,')

    new_lines.append(line)

with open('crates/memfuse-index/src/hnsw.rs', 'w') as f:
    f.writelines(new_lines)
