from typing import List, Dict, Any, Optional
from .interface import VectorStoreInterface, GraphInterface
import logging
import json
import os
from sentence_transformers import SentenceTransformer
import redis.asyncio as redis

logger = logging.getLogger(__name__)

class AgenticMemoryManager:
    """
    Orchestrates storage and retrieval of episodic and semantic memory.
    Uses DaaB interfaces (VectorStore, Graph) and Redis for short-term memory.
    """
    _model = None
    _redis = None

    def __init__(self, workspace_id: str):
        self.workspace_id = workspace_id
        if AgenticMemoryManager._model is None:
             logger.info("Loading embedding model...")
             try:
                 AgenticMemoryManager._model = SentenceTransformer('all-MiniLM-L6-v2')
             except Exception as e:
                 logger.error(f"Failed to load embedding model: {e}")
        
        if AgenticMemoryManager._redis is None:
            redis_url = os.environ.get("REDIS_URL", "redis://localhost:6379")
            AgenticMemoryManager._redis = redis.from_url(redis_url, decode_responses=True)

    def _get_embedding(self, text: str) -> List[float]:
        if AgenticMemoryManager._model is None:
            raise RuntimeError("Embedding model not loaded")
        return AgenticMemoryManager._model.encode(text).tolist()

    async def add_short_term_memory(self, key: str, value: Any, ttl: int = 3600):
        """Stores data in Redis with TTL."""
        try:
            if isinstance(value, (dict, list)):
                value = json.dumps(value)
            await self._redis.setex(f"stm:{self.workspace_id}:{key}", ttl, value)
        except Exception as e:
            logger.error(f"Failed to add short-term memory: {e}")

    async def get_short_term_memory(self, key: str) -> Optional[Any]:
        """Retrieves data from Redis."""
        try:
            val = await self._redis.get(f"stm:{self.workspace_id}:{key}")
            if val:
                try:
                    return json.loads(val)
                except json.JSONDecodeError:
                    return val
            return None
        except Exception as e:
            logger.error(f"Failed to get short-term memory: {e}")
            return None

    async def add_episodic_memory(self, content: str, metadata: Dict[str, Any] = {}):
        """
        Stores an episodic memory (e.g., conversation turn, action).
        """
        try:
            embedding = self._get_embedding(content)
            
            await VectorStoreInterface.add_document(
                content=content,
                embedding=embedding,
                source_file="episodic_memory",
                metadata={**metadata, "type": "episodic"},
                namespace=self.workspace_id # Pass workspace_id as namespace
            )
        except Exception as e:
            logger.error(f"Failed to add episodic memory: {e}")

    async def add_semantic_memory(self, content: str, metadata: Dict[str, Any] = {}):
        """
        Stores a semantic memory (e.g., fact, knowledge).
        """
        try:
            embedding = self._get_embedding(content)
            
            await VectorStoreInterface.add_document(
                content=content,
                embedding=embedding,
                source_file="semantic_memory",
                metadata={**metadata, "type": "semantic"},
                namespace=self.workspace_id # Pass workspace_id as namespace
            )
        except Exception as e:
            logger.error(f"Failed to add semantic memory: {e}")

    async def retrieve_context(self, query: str, limit: int = 5) -> List[Dict]:
        """
        Retrieves relevant context based on query.
        Implements Hierarchical Search: Decides between Local (App) and Global DaaB.
        """
        try:
            from engine.knowledge_router import KnowledgeRouter
            router = KnowledgeRouter()
            
            # 1. Determine Scope
            routing_info = await router.determine_scope(query)
            schemas = routing_info["schemas"]
            
            embedding = self._get_embedding(query)
            all_results = []
            
            # 2. Iterate through determined schemas
            for schema in schemas:
                # 'current' maps to the local workspace_id (app schema)
                target_schema = self.workspace_id if schema == "current" else schema
                
                logger.info(f"Retrieving context from schema: {target_schema}")
                results = await VectorStoreInterface.search(
                    embedding=embedding,
                    limit=limit,
                    namespace=target_schema
                )
                all_results.extend(results)
            
            # 3. Sort by similarity across all results
            all_results.sort(key=lambda x: x.get("similarity", 0), reverse=True)
            
            return all_results[:limit]
            
        except Exception as e:
            logger.error(f"Failed to retrieve hierarchical context: {e}")
            return []

    async def get_related_entities(self, entity_name: str) -> List[Any]:
        """
        Retrieves related entities from the Knowledge Graph.
        """
        try:
            # Use parameterized query
            # Note: In AGE, parameters in the cypher string are prefixed with $
            cypher = "MATCH (n:Entity {name: $name})-[r]-(m) RETURN m"
            params = {"name": entity_name}
            return await GraphInterface.execute_cypher(cypher, params)
        except Exception as e:
            logger.error(f"Failed to get related entities: {e}")
            return []
