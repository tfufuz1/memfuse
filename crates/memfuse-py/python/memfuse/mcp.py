import logging
from typing import Any, Dict, List, Optional
from fastmcp import FastMCP
from . import open as memfuse_open

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("memfuse-mcp")

def create_mcp_server(db_path: str, dimension: int = 1536) -> FastMCP:
    """
    Creates a FastMCP server instance for a MemFuse database.
    """
    mcp = FastMCP("MemFuse")
    
    # We open the DB lazily or at startup
    # For now, let's assume we open it once.
    try:
        db = memfuse_open(db_path, dimension=dimension)
    except Exception as e:
        logger.error(f"Failed to open MemFuse database at {db_path}: {e}")
        raise

    @mcp.tool()
    def memfuse_search(
        query: str,
        collection: str = "default",
        k: int = 10,
        vector: Optional[List[float]] = None
    ) -> List[Dict[str, Any]]:
        """
        Hybrid search across stored documents (vector + BM25 + metadata).
        
        Args:
            query: Natural language query (used for BM25)
            collection: Collection name
            k: Number of results to return
            vector: Optional embedding vector for semantic search.
                    If omitted, a zero-vector is used (BM25 focus).
        """
        try:
            col = db.collection(collection)
            import numpy as np
            
            if vector is not None:
                v = np.array(vector, dtype=np.float32)
            else:
                # Still using zero-vector if no vector provided,
                # but now it's explicit and controllable by the caller.
                v = np.zeros(dimension, dtype=np.float32)

            results = col.hybrid_search(query, v, k)
            return [
                {
                    "id": r.id,
                    "score": r.score,
                    "metadata": r.metadata
                }
                for r in results
            ]
        except Exception as e:
            logger.error(f"Error in memfuse_search: {e}")
            return [{"error": str(e)}]

    @mcp.tool()
    def memfuse_get(id: str, collection: str = "default") -> Optional[Dict[str, Any]]:
        """
        Retrieve a specific document by ID.
        """
        try:
            col = db.collection(collection)
            doc = col.get(id)
            if doc:
                return {"id": doc.id, "metadata": doc.metadata}
            return None
        except Exception as e:
            logger.error(f"Error in memfuse_get: {e}")
            return {"error": str(e)}

    @mcp.tool()
    def memfuse_insert(
        id: str,
        text: str,
        collection: str = "default",
        metadata: Optional[Dict[str, Any]] = None,
        vector: Optional[List[float]] = None
    ) -> str:
        """
        Store a document with embedding and metadata.

        Args:
            id: Unique document identifier
            text: Text content for BM25 indexing
            collection: Target collection
            metadata: Additional JSON metadata
            vector: Embedding vector. Required if no auto-embedding provider is configured.
        """
        try:
            col = db.collection(collection)
            import numpy as np

            if vector is not None:
                v = np.array(vector, dtype=np.float32)
            else:
                # FR-7.3-003: "Ohne Embedding-Provider muss der Client Vektoren mitliefern."
                # We return an error if no vector is provided, rather than silent zero-spoofing.
                return "Error: Embedding vector is required (auto-embedding not yet configured)"
            
            # Store text in metadata so it's searchable by BM25
            meta = metadata or {}
            meta["text"] = text
            
            col.insert(id, v, meta)
            return f"Document {id} inserted successfully"
        except Exception as e:
            logger.error(f"Error in memfuse_insert: {e}")
            return f"Error: {e}"

    @mcp.tool()
    def memfuse_collections() -> List[str]:
        """
        List all available collections.
        """
        try:
            return db.list_collections()
        except Exception as e:
            logger.error(f"Error in memfuse_collections: {e}")
            return [f"Error: {e}"]

    @mcp.resource("memfuse://stats")
    def memfuse_stats() -> str:
        """
        Database statistics (doc count, memory usage, index health).
        """
        try:
            stats = db.stats()
            return str(stats)
        except Exception as e:
            return f"Error retrieving stats: {e}"

    return mcp

if __name__ == "__main__":
    import sys
    path = sys.argv[1] if len(sys.argv) > 1 else "./memfuse_db"
    server = create_mcp_server(path)
    server.run()
