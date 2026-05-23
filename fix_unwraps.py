import sys

def process_file(filepath, lines_to_annotate):
    with open(filepath, 'r') as f:
        lines = f.readlines()

    new_lines = []
    for i, line in enumerate(lines):
        line_num = i + 1
        if line_num in lines_to_annotate:
            if '// unwrap allowed' not in line:
                new_line = line.rstrip() + ' // unwrap allowed\n'
                new_lines.append(new_line)
            else:
                new_lines.append(line)
        else:
            new_lines.append(line)

    with open(filepath, 'w') as f:
        f.writelines(new_lines)

if __name__ == "__main__":
    # memfuse-checkpoint
    process_file('crates/memfuse-checkpoint/src/lib.rs', [37, 42, 194, 199, 204, 224, 228, 232, 249, 255, 258, 271, 273, 285, 289, 293, 295, 310, 314, 331])
    # memfuse-text
    process_file('crates/memfuse-text/src/inverted.rs', [532, 537, 542, 562])
