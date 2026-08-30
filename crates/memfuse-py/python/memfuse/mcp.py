"""
MCP (Model Context Protocol) integration for MemFuse using FastMCP.
"""
from typing import Optional, List, Dict, Any
import memfuse

def create_mcp_server(db_path: str, dimension: int = 1536, name: str = "MemFuse"):
    from fastmcp import FastMCP
    mcp = FastMCP(name)

    @mcp.tool()
    async def memfuse_insert(id: str, text: str, collection: str = "default", metadata: Optional[Dict[str, Any]] = None) -> str:
        """Insert a document into MemFuse."""
        db = memfuse.open(db_path, dimension=dimension)
        # Dummy vector for text insertion in Python stub/wrapper if embedding model isn't active
        import numpy as np
        vector = np.zeros(dimension, dtype=np.float32)
        col = db.collection(collection)
        meta = metadata or {}
        meta["text"] = text
        col.insert(id, vector, meta)
        return f"Document '{id}' inserted successfully into collection '{collection}'."

    @mcp.tool()
    async def memfuse_search(query: str, collection: str = "default", k: int = 5) -> List[Dict[str, Any]]:
        """Search documents in MemFuse."""
        db = memfuse.open(db_path, dimension=dimension)
        import numpy as np
        vector = np.zeros(dimension, dtype=np.float32)
        col = db.collection(collection)
        results = col.search(vector, k=k)
        return [{"id": r.id, "score": r.score, "metadata": r.metadata} for r in results]

    @mcp.tool()
    async def memfuse_get(id: str, collection: str = "default") -> Optional[Dict[str, Any]]:
        """Get a document by ID."""
        db = memfuse.open(db_path, dimension=dimension)
        col = db.collection(collection)
        doc = col.get(id)
        if doc:
            return {"id": doc.id, "metadata": doc.metadata}
        return None

    @mcp.tool()
    async def memfuse_collections() -> List[str]:
        """List all collections."""
        db = memfuse.open(db_path, dimension=dimension)
        return db.list_collections()

    @mcp.resource("memfuse://stats")
    async def memfuse_stats() -> str:
        """Get database statistics."""
        db = memfuse.open(db_path, dimension=dimension)
        return str(db.stats())

    return mcp
