#!/usr/bin/env python3
"""
Python wrapper entry-point for memfuse-mcp-server binary.
Enables running `memfuse-mcp` seamlessly via `uvx memfuse-mcp`.
"""

import os
import sys
import shutil
import subprocess
from pathlib import Path

def find_repo_root() -> Path:
    """Recursively search upward from current file to find repository root containing Cargo.toml."""
    curr = Path(__file__).resolve().parent
    for p in [curr] + list(curr.parents):
        if (p / "Cargo.toml").exists() and (p / "crates" / "memfuse-mcp").exists():
            return p
    return curr

def find_mcp_binary() -> str:
    # 1. Check explicit environment override
    env_bin = os.environ.get("MEMFUSE_MCP_BINARY")
    if env_bin and os.path.isfile(env_bin) and os.access(env_bin, os.X_OK):
        return env_bin

    # 2. Check system PATH
    which_bin = shutil.which("memfuse-mcp-server")
    if which_bin:
        return which_bin

    # 3. Check target directory relative to repo root or current working dir
    repo_root = find_repo_root()
    candidates = [
        repo_root / "target" / "release" / "memfuse-mcp-server",
        repo_root / "target" / "debug" / "memfuse-mcp-server",
        Path.cwd() / "target" / "release" / "memfuse-mcp-server",
        Path.cwd() / "target" / "debug" / "memfuse-mcp-server",
    ]

    for candidate in candidates:
        if candidate.exists() and os.access(candidate, os.X_OK):
            return str(candidate)

    # 4. Attempt auto-compilation via cargo if in source tree
    if (repo_root / "Cargo.toml").exists():
        sys.stderr.write("[memfuse-mcp] memfuse-mcp-server binary not found. Building release binary via Cargo...\n")
        sys.stderr.flush()
        cmd = ["cargo", "build", "--release", "-p", "memfuse-mcp", "--bin", "memfuse-mcp-server"]
        res = subprocess.run(cmd, cwd=str(repo_root))
        if res.returncode == 0:
            target_bin = repo_root / "target" / "release" / "memfuse-mcp-server"
            if target_bin.exists():
                return str(target_bin)

    raise FileNotFoundError(
        "Could not locate or build `memfuse-mcp-server` executable.\n"
        "Please ensure `memfuse-mcp-server` is installed in system PATH, "
        "or set `MEMFUSE_MCP_BINARY` environment variable pointing to the binary."
    )

def main():
    try:
        binary_path = find_mcp_binary()
    except Exception as e:
        sys.stderr.write(f"Error: {e}\n")
        sys.exit(1)

    args = [binary_path] + sys.argv[1:]

    # Use os.execv on POSIX for zero-overhead process replacement
    if hasattr(os, "execv"):
        try:
            os.execv(binary_path, args)
        except OSError:
            pass

    # Fallback to subprocess for platforms/environments where execv fails or isn't available
    res = subprocess.run(args)
    sys.exit(res.returncode)

if __name__ == "__main__":
    main()
