path = "crates/memfuse-store/src/checkpoint.rs"
with open(path, "r") as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if ".unwrap() // unwrap allowed" in line:
        if ";" not in line and "assert" not in line:
             # Try to find if it needs a semicolon
             line = line.replace(".unwrap() // unwrap allowed", ".unwrap(); // unwrap allowed")
    new_lines.append(line)

with open(path, "w") as f:
    f.writelines(new_lines)
