import sys
import os
import asyncio
import logging
import json
from pathlib import Path
from typing import Optional, Dict, Any, List

# Setup Logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("atlas-rag")

# Path Setup
ROOT_DIR = Path(__file__).parent.parent.parent.parent.resolve()
AGENTS_DIR = ROOT_DIR / "apps" / "kernel" / "src_agents"
sys.path.append(str(AGENTS_DIR))
sys.path.append(str(ROOT_DIR / "apps" / "kernel"))

# Import DaaB
try:
    from daab.core import DaaB
    HAS_DAAB = True
except ImportError as e:
    logger.error(f"Failed to import DaaB: {e}")
    HAS_DAAB = False

# Mock DaaB Implementation
class MockDaaB:
    def __init__(self, db_path=None):
        self.memories = []
        logger.warning("Running in MOCK DaaB mode due to missing dependencies.")

    async def add_memory(self, content, agent_id="default", metadata=None):
        mid = f"mock-{len(self.memories)}"
        self.memories.append({"id": mid, "content": content, "agent_id": agent_id, "metadata": metadata or {}})
        return mid

    async def search_memories(self, query, limit=5, agent_id=None):
        results = []
        for m in self.memories:
            if query.lower() in m["content"].lower():
                res = m.copy()
                res["score"] = 0.99
                results.append(res)
        return results[:limit]

    async def search_hybrid(self, query, limit=5, agent_id=None):
        return await self.search_memories(query, limit, agent_id)

# Initialize DaaB
db_path = os.getenv("MCP_RAG_DB")
daab_instance = None
if HAS_DAAB:
    try:
        daab_instance = DaaB(db_path=db_path)
    except Exception as e:
        logger.error(f"Failed to initialize DaaB: {e}")
        daab_instance = MockDaaB()
else:
    daab_instance = MockDaaB()

# Import MCP
try:
    from mcp.server.fastmcp import FastMCP
    HAS_FASTMCP = True
except ImportError:
    HAS_FASTMCP = False
    from mcp.server import Server
    import mcp.types as types
    from mcp.server.stdio import stdio_server

# --- FastMCP Implementation ---
if HAS_FASTMCP:
    mcp = FastMCP("Atlas RAG")

    @mcp.tool()
    async def add_memory(content: str, agent_id: str = "default", metadata: dict = None) -> str:
        """Stores a text memory in the semantic database."""
        if not daab_instance:
            return "Error: DaaB not initialized."
        try:
            mid = await daab_instance.add_memory(content, agent_id, metadata)
            return f"Memory stored with ID: {mid}"
        except Exception as e:
            return f"Error adding memory: {str(e)}"

    @mcp.tool()
    async def search_memory(query: str, limit: int = 5, agent_id: str = None) -> str:
        """Semantically searches for memories."""
        if not daab_instance:
            return "Error: DaaB not initialized."
        try:
            results = await daab_instance.search_memories(query, limit, agent_id)
            return json.dumps(results, indent=2)
        except Exception as e:
            return f"Error searching memory: {str(e)}"

    @mcp.tool()
    async def hybrid_search(query: str, limit: int = 5, agent_id: str = None) -> str:
        """Performs hybrid search (Vector + Keyword)."""
        if not daab_instance:
            return "Error: DaaB not initialized."
        try:
            results = await daab_instance.search_hybrid(query, limit, agent_id)
            return json.dumps(results, indent=2)
        except Exception as e:
            return f"Error in hybrid search: {str(e)}"

    if __name__ == "__main__":
        mcp.run()

# --- Standard Server Implementation (Fallback) ---
else:
    server = Server("atlas-rag")

    @server.list_tools()
    async def list_tools() -> list[types.Tool]:
        return [
            types.Tool(
                name="add_memory",
                description="Stores a text memory in the semantic database.",
                inputSchema={
                    "type": "object",
                    "properties": {
                        "content": {"type": "string"},
                        "agent_id": {"type": "string"},
                        "metadata": {"type": "object"}
                    },
                    "required": ["content"]
                }
            ),
            types.Tool(
                name="search_memory",
                description="Semantically searches for memories.",
                inputSchema={
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "limit": {"type": "integer"},
                        "agent_id": {"type": "string"}
                    },
                    "required": ["query"]
                }
            )
        ]

    @server.call_tool()
    async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
        if not daab_instance:
            return [types.TextContent(type="text", text="Error: DaaB not initialized.")]

        if name == "add_memory":
            mid = await daab_instance.add_memory(
                arguments.get("content"),
                arguments.get("agent_id", "default"),
                arguments.get("metadata", {})
            )
            return [types.TextContent(type="text", text=f"Memory stored with ID: {mid}")]
        
        elif name == "search_memory":
            results = await daab_instance.search_memories(
                arguments.get("query"),
                arguments.get("limit", 5),
                arguments.get("agent_id")
            )
            return [types.TextContent(type="text", text=json.dumps(results))]
            
        raise ValueError(f"Unknown tool: {name}")

    async def main():
        async with stdio_server() as (read, write):
            await server.run(read, write, server.create_initialization_options())

    if __name__ == "__main__":
        asyncio.run(main())