path = "crates/memfuse-core/src/types/budget.rs"
with open(path, "r") as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if "match result.err().unwrap() {" in line:
        new_lines.append("        let err = result.err().unwrap(); // unwrap allowed\n")
        new_lines.append("        match err {\n")
    else:
        new_lines.append(line)

with open(path, "w") as f:
    f.writelines(new_lines)
