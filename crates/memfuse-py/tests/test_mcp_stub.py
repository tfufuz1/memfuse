import pytest
import os
import sys

# Add the python directory to sys.path so we can import memfuse
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "../python")))

def test_mcp_import():
    from memfuse.mcp import create_mcp_server
    assert create_mcp_server is not None

def test_fastmcp_import():
    from fastmcp import FastMCP
    assert FastMCP is not None
