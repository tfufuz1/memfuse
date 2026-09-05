"""
Atlas MCP Gateway RAG Server — MemFuse Adapter.

Drop-in enhancement for `atlas/apps/mcp-gateway/servers/atlas_rag.py`.
Provides tool endpoints for Atlas AI Agents using MemFuse as the
high-performance memory and context engine.
"""

import asyncio
import json
import logging
import os
from typing import Any, Dict, List, Optional
from mcp.server.fastmcp import FastMCP

from .memfuse_daab_provider import MemFuseDaaBProvider

logger = logging.getLogger("atlas.mcp.memfuse_rag")

mcp = FastMCP("Atlas-MemFuse-RAG")
provider = MemFuseDaaBProvider()


@mcp.tool()
async def remember_context(
    content: str,
    agent_id: str = "default_agent",
    importance: float = 1.0,
    tags: Optional[List[str]] = None,
) -> str:
    """
    Persists knowledge, facts, or agent decisions into MemFuse.
    The entry is indexed via vector embedding, BM25 keywords, and graph topology.
    """
    try:
        metadata = {"tags": tags or []}
        memory_id = await provider.add_memory(
            content=content,
            agent_id=agent_id,
            metadata=metadata,
            importance_score=importance,
        )
        return f"Successfully remembered in MemFuse with Memory ID: {memory_id}"
    except Exception as e:
        logger.error(f"Error persisting context into MemFuse: {e}")
        return f"Error: Failed to remember context: {str(e)}"


@mcp.tool()
async def retrieve_context(
    query: str,
    agent_id: Optional[str] = None,
    limit: int = 5,
) -> str:
    """
    Performs 4-signal hybrid retrieval with Reciprocal Rank Fusion (k=60)
    over stored agent memories in MemFuse.
    """
    try:
        results = await provider.search_memories(
            query=query,
            agent_id=agent_id,
            limit=limit,
        )
        if not results:
            return "No relevant memories found in MemFuse."

        output = []
        for i, hit in enumerate(results, 1):
            score = hit.get("score", 0.0)
            text = hit.get("content", "")
            meta = hit.get("metadata", {})
            output.append(f"[{i}] (Score: {score:.4f}) {text}\n    Meta: {json.dumps(meta)}")

        return "\n\n".join(output)
    except Exception as e:
        logger.error(f"Error querying MemFuse context: {e}")
        return f"Error: Failed to retrieve context: {str(e)}"


if __name__ == "__main__":
    mcp.run()
