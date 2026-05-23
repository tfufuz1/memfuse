import os

def scan_file(path):
    with open(path, "r") as f:
        content = f.read()

    # Split by lines to keep track of line numbers
    lines = content.splitlines()
    in_test_block = False

    for i, line in enumerate(lines):
        line_num = i + 1
        # Check if we entered a test block
        if "#[cfg(test)]" in line or "mod tests" in line:
            in_test_block = True

        if not in_test_block:
            if (".unwrap()" in line or ".expect(" in line) and "// unwrap allowed" not in line:
                print(f"{path}:{line_num}: {line.strip()}")

for root, dirs, files in os.walk("crates"):
    if "src" not in root: continue
    for file in files:
        if file.endswith(".rs"):
            scan_file(os.path.join(root, file))
