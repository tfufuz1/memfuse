from typing import List, Dict, Any
from .db_manager import DatabaseManager
import logging

logger = logging.getLogger(__name__)

class HybridSearchProvider:
    """
    Combines Vector Search (Semantic) with Graph Search (Relational/Knowledge).
    Uses Reciprocal Rank Fusion (RRF) to merge results.
    """
    
    RRF_K = 60 # Default constant for RRF

    @classmethod
    async def search(cls, query_embedding: List[float], query_text: str, limit: int = 10) -> List[Dict[str, Any]]:
        """
        Performs hybrid search.
        
        Args:
            query_embedding: Vector representation of the query.
            query_text: Raw text for graph matching (optional) or full-text search.
            limit: Number of results to return.
        """
        
        # 1. Run Vector Search
        vector_results = await cls._vector_search(query_embedding, limit * 2)
        
        # 2. Run Graph Search (e.g. finding nodes connected to keywords in query)
        # For this prototype, we assume we extract keywords or entities from query_text
        # and find 2-hop neighbors in the graph.
        graph_results = await cls._graph_search(query_text, limit * 2)
        
        # 3. Fuse Results
        fused = cls._rrf_fusion(vector_results, graph_results)
        
        return fused[:limit]

    @classmethod
    async def _vector_search(cls, embedding: List[float], limit: int) -> List[Dict[str, Any]]:
        query = """
            SELECT id, content, metadata, 1 - (embedding <=> $1) as score
            FROM knowledge_chunks
            ORDER BY score DESC
            LIMIT $2
        """
        try:
            records = await DatabaseManager.fetch(query, embedding, limit)
            return [dict(r) for r in records]
        except Exception as e:
            logger.error(f"Vector search failed: {e}")
            return []

    @classmethod
    async def _graph_search(cls, text: str, limit: int) -> List[Dict[str, Any]]:
        # Placeholder for complex graph logic using Apache AGE.
        # We assume we have a way to match text to nodes.
        # Example cypher: MATCH (n:Entity) WHERE n.name CONTAINS $text RETURN n
        # This is simplified.
        
        # IMPORTANT: AGE requires cypher function call.
        # 'text' should be sanitized or passed safely.
        # Ideally we use parameters, but AGE python support might be tricky with params in cypher call depending on driver.
        
        cypher = """
            SELECT * FROM cypher('knowledge_graph', $$
                MATCH (n:Entity) 
                WHERE n.name CONTAINS %s
                RETURN n
                LIMIT %s
            $$) as (n agtype);
        """
        # Note: Robust parameter injection for AGE needed here. 
        # This is a stub implementation.
        return []

    @classmethod
    def _rrf_fusion(cls, list_a: List[Dict[str, Any]], list_b: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """
        Reciprocal Rank Fusion.
        """
        scores = {}
        
        # Process Vector Results
        for rank, item in enumerate(list_a):
            key = str(item['id'])
            scores[key] = scores.get(key, 0) + (1.0 / (cls.RRF_K + rank + 1))
            item['_source'] = 'vector' # Tag source if new
            if key not in [str(x['id']) for x in list_b]: # Keep item if only in vector
                 pass # Logic to merge item data if needed
        
        # Process Graph Results
        for rank, item in enumerate(list_b):
            key = str(item.get('id', 'unknown')) # Graph nodes might not have same ID structure
            scores[key] = scores.get(key, 0) + (1.0 / (cls.RRF_K + rank + 1))
        
        # Sort by fused score
        # Note: We need to reconstruct the full object list.
        # This is a simplified merge.
        
        combined_map = {str(i['id']): i for i in list_a}
        for i in list_b:
             combined_map[str(i.get('id'))] = i
             
        results = []
        for key, score in sorted(scores.items(), key=lambda x: x[1], reverse=True):
            if key in combined_map:
                obj = combined_map[key]
                obj['score'] = score
                results.append(obj)
                
        return results
