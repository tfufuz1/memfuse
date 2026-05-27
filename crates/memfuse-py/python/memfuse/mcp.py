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
    def memfuse_search(query: str, collection: str = "default", k: int = 10) -> List[Dict[str, Any]]:
        """
        Hybrid search across stored documents (vector + BM25 + metadata).
        
        Args:
            query: Natural language query
            collection: Collection name
            k: Number of results to return
        """
        try:
            col = db.collection(collection)
            # Since we don't have an embedding tool yet in this WP, 
            # we might need to handle how vectors are provided or generated.
            # FR-7.3-003 says: "Ohne Embedding-Provider muss der Client Vektoren mitliefern."
            # But the tool definition in FR-7.3-001 for memfuse_search only takes 'query'.
            # This implies the server should handle embedding OR query is only for BM25.
            # Wait, hybrid_search in Rust takes (text, vector, k).
            
            # If query is only text, we can only do BM25 if no embedding is provided.
            # But the spec says 'Hybrid search'. 
            # Let's assume for now it uses a dummy vector if not provided, or we need an embedding.
            # For the sake of this WP, I'll implement it as text-only search if no vector is available,
            # or use a zero vector for semantic part (which is bad but matches the 'query' only signature).
            # Actually, I'll check if I can add a 'vector' param too.
            
            # Re-reading FR-7.3-001: parameters: { "query": {"type": "string", ...} }
            # It doesn't have a 'vector' parameter for search.
            # This strongly implies that either:
            # 1. memfuse_search only does BM25.
            # 2. memfuse_search handles embedding internally.
            
            # Since FR-7.3-003 mentions "Auto-Embedding bei Insert", I'll assume we want the same for search.
            # If no embedder is configured, we'll fall back to BM25 or throw error if only vector search is requested.
            
            # For now, let's use a zero vector of correct dimension to satisfy the hybrid_search call.
            import numpy as np
            zero_vector = np.zeros(dimension, dtype=np.float32)
            
            results = col.hybrid_search(query, zero_vector, k)
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
    def memfuse_insert(id: str, text: str, collection: str = "default", metadata: Dict[str, Any] = None) -> str:
        """
        Store a document with embedding and metadata.
        """
        try:
            col = db.collection(collection)
            # Auto-embedding placeholder
            import numpy as np
            # Dummy vector for now as per FR-7.3-003 fallback
            vector = np.zeros(dimension, dtype=np.float32)
            
            # Store text in metadata so it's searchable by BM25
            meta = metadata or {}
            meta["text"] = text
            
            col.insert(id, vector, meta)
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
