import pytest
import numpy as np
import os
import shutil
import asyncio
from memfuse.mcp import create_mcp_server

@pytest.fixture
def db_path(tmp_path):
    path = str(tmp_path / "mcp_test_db")
    yield path
    if os.path.exists(path):
        shutil.rmtree(path)

def run_async(coro):
    return asyncio.run(coro)

def test_mcp_tools_registration(db_path):
    mcp = create_mcp_server(db_path, dimension=4)
    
    # Use the public API to check tools (it's async)
    tools = run_async(mcp.list_tools())
    tool_names = [tool.name for tool in tools]
    assert "memfuse_search" in tool_names
    assert "memfuse_get" in tool_names
    assert "memfuse_insert" in tool_names
    assert "memfuse_collections" in tool_names

def test_mcp_insert_and_search(db_path):
    mcp = create_mcp_server(db_path, dimension=4)
    
    # Use call_tool which is async
    res = run_async(mcp.call_tool("memfuse_insert", {"id": "doc1", "text": "rust is awesome", "collection": "test"}))
    assert "inserted successfully" in str(res)
    
    # Search
    results = run_async(mcp.call_tool("memfuse_search", {"query": "rust", "collection": "test", "k": 1}))
    assert len(str(results)) > 0
    assert "doc1" in str(results)

def test_mcp_stats_resource(db_path):
    mcp = create_mcp_server(db_path, dimension=4)
    
    # Check resources (it's async)
    resources = run_async(mcp.list_resources())
    resource_names = [res.name for res in resources]
    assert "memfuse_stats" in resource_names
    
    stats_output = run_async(mcp.read_resource("memfuse://stats"))
    assert "DbStats" in str(stats_output)
