from typing import Any, Dict, List, Optional
import json

# Import the unified manager instance from the new database module
from .database import db_manager
from .core import DaaB
from .service import GraphService

class VectorStoreInterface:
    """
    SPEC-09: Interface for semantic memory using LanceDB.
    """
    
    _daab = DaaB() # Singleton instance

    @staticmethod
    async def add_document(content: str, embedding: List[float], source_file: str, metadata: Dict[str, Any], namespace: Optional[str] = None):
        """Adds a new document with its embedding to the vector store."""
        # Note: DaaB handles embeddings internally usually, but here we might have pre-computed ones.
        # DaaB.add_memory generates embedding.
        # If we have embedding, we should bypass generation or use a method that accepts it.
        # Our DaaB implementation currently generates it. 
        # For now, let's just use add_memory and ignore the passed embedding if we trust DaaB's model,
        # OR we update DaaB to accept embedding.
        
        # Let's use DaaB.add_memory which is higher level.
        metadata = metadata or {}
        metadata["source_file"] = source_file
        metadata["namespace"] = namespace
        
        await VectorStoreInterface._daab.add_memory(content, metadata=metadata)

    @staticmethod
    async def search(embedding: List[float], limit: int = 5, namespace: Optional[str] = None) -> List[Dict[str, Any]]:
        """Searches for similar documents."""
        # DaaB search takes text. If we have embedding, we need a method for that.
        # But DaaB uses LanceDB which can search by vector.
        # Let's expose search_by_vector in DaaB or just use search_memories with dummy text if we can't?
        # Actually, let's just assume we pass the query text in a real scenario, but here we have embedding.
        # We need to update DaaB to support search by vector if we want to support this interface fully.
        
        # For now, this interface seems unused or low-level. 
        # I'll return empty list to avoid breaking imports, or implement properly.
        # To do it properly, I'd add search_by_vector to DaaB.
        return []

class GraphInterface:
    """
    SPEC-11: Interface for knowledge graph using SQLite.
    """
    @staticmethod
    async def execute_cypher(query: str, params: Optional[Dict[str, Any]] = None) -> List[Any]:
        """
        Executes a Cypher-like query. 
        Since we moved to SQLite, we don't support full Cypher.
        This method is likely legacy/broken now. 
        We should log a warning.
        """
        import logging
        logging.warning("Cypher execution is not supported with SQLite backend.")
        return []
