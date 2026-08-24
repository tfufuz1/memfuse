#!/usr/bin/env nix-shell
#!nix-shell -p python311 python311Packages.venvShellHook python311Packages.pip maturin cargo rustc -i bash
python3 -m venv .venv
source .venv/bin/activate
pip install maturin pytest pytest-asyncio numpy
maturin develop
pytest tests/
