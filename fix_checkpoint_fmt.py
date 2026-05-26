path = "crates/memfuse-store/src/checkpoint.rs"
with open(path, "r") as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if "assert_eq!(storage.get(b\"key3\").await.unwrap()" in line:
        new_lines.append("        let val = storage.get(b\"key3\").await.unwrap(); // unwrap allowed\n")
        new_lines.append("        assert_eq!(val, Some(b\"val3\".to_vec()));\n")
    else:
        new_lines.append(line)

with open(path, "w") as f:
    f.writelines(new_lines)
