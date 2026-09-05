# src-python/daab/core.py
"""
Core implementation of the "Database as a Brain" (DaaB) module.

This module provides the DaaB class, which serves as the primary interface
for managing agent memories using LanceDB for vector storage and SQLite for metadata.
"""

import asyncio
import os
import logging
from typing import Any, Dict, List, Optional
from pathlib import Path

import lancedb
import pyarrow as pa
import numpy as np
import json
import time
import uuid
from sentence_transformers import SentenceTransformer
from .database import db_manager

# --- Constants ---
EMBEDDING_MODEL = "all-MiniLM-L6-v2"
EMBEDDING_DIM = 384

logger = logging.getLogger(__name__)

class DaaB:
    """
    Database as a Brain: Manages agent memories, including semantic search using LanceDB
    and keyword search using SQLite FTS5.
    """

    def __init__(self, db_path: Optional[str] = None):
        """
        Initializes the DaaB instance.

        Args:
            db_path (Optional[str]): Path to the LanceDB directory.
                If None, defaults will be loaded from environment variables.
        """
        if db_path:
            self.db_path = db_path
        else:
            self.db_path = os.getenv("LANCEDB_PATH", str(Path.home() / ".atlas" / "data" / "vectors"))
        
        self._db = None
        self._table = None
        self._model: Optional[SentenceTransformer] = None
        logger.info(f"DaaB initialized with path: {self.db_path}")

    def connect(self):
        """Establishes the LanceDB connection."""
        if not self._db:
            try:
                os.makedirs(self.db_path, exist_ok=True)
                self._db = lancedb.connect(self.db_path)
                
                # Define schema
                schema = pa.schema([
                    pa.field("id", pa.string()),
                    pa.field("agent_id", pa.string()),
                    pa.field("content", pa.string()),
                    pa.field("vector", pa.list_(pa.float32(), EMBEDDING_DIM)),
                    pa.field("metadata", pa.string()), # JSON string
                    pa.field("created_at", pa.float64()) # Timestamp
                ])
                
                # Create or open table
                try:
                    self._table = self._db.open_table("memories")
                except FileNotFoundError:
                     self._table = self._db.create_table("memories", schema=schema)
                
                logger.info("LanceDB connection established and table opened.")
            except Exception as e:
                logger.error(f"Error connecting to LanceDB: {e}")
                raise

    def close(self):
        """Closes the database connection (No-op for LanceDB)."""
        pass

    def _get_embedding(self, text: str) -> np.ndarray:
        """
        Generates a vector embedding for the given text.
        """
        if self._model is None:
            logger.info(f"Loading embedding model: {EMBEDDING_MODEL}...")
            self._model = SentenceTransformer(EMBEDDING_MODEL)
            logger.info("Embedding model loaded.")
        return self._model.encode(text)

    async def add_memory(
        self,
        content: str,
        agent_id: str = "default",
        metadata: Optional[Dict[str, Any]] = None,
    ) -> str:
        """
        Adds a new memory to both LanceDB (vector) and SQLite (FTS5).
        """
        if not self._table:
            self.connect()

        logger.info(f"Adding memory for agent '{agent_id}': '{content[:50]}...'")
        embedding = self._get_embedding(content)
        
        memory_id = str(uuid.uuid4())
        meta_json = json.dumps(metadata or {})
        
        # 1. Add to LanceDB
        data = [{
            "id": memory_id,
            "agent_id": agent_id,
            "content": content,
            "vector": embedding.tolist(),
            "metadata": meta_json,
            "created_at": time.time()
        }]
        self._table.add(data)
        
        # 2. Add to SQLite FTS5 for keyword search
        try:
            await db_manager.execute(
                "INSERT INTO knowledge_fts (id, content, metadata) VALUES (?, ?, ?)",
                memory_id, content, meta_json
            )
        except Exception as e:
            logger.error(f"Failed to add memory to SQLite FTS: {e}")
            # Non-critical, we still have the vector search

        logger.info(f"Memory added with ID: {memory_id}")
        return memory_id

    async def search_memories(
        self, query_text: str, limit: int = 5, agent_id: Optional[str] = None
    ) -> List[Dict[str, Any]]:
        """
        Searches for relevant memories using semantic similarity (LanceDB).
        """
        if not self._table:
            self.connect()

        logger.info(f"Searching memories with query: '{query_text[:50]}...'")
        query_embedding = self._get_embedding(query_text)

        # LanceDB Search
        search = self._table.search(query_embedding).limit(limit)
        
        if agent_id:
            search = search.where(f"agent_id = '{agent_id}'", prefilter=True)
            
        results = search.to_pandas()
        
        mapped_results = []
        for _, row in results.iterrows():
            mapped_results.append({
                "id": row["id"],
                "agent_id": row["agent_id"],
                "content": row["content"],
                "metadata": json.loads(row["metadata"]),
                "score": 1 - row["_distance"] # Approximate similarity
            })

        logger.info(f"Found {len(mapped_results)} relevant memories.")
        return mapped_results

    async def search_hybrid(
        self, query_text: str, limit: int = 5, agent_id: Optional[str] = None
    ) -> List[Dict[str, Any]]:
        """
        Performs hybrid search by combining vector results and keyword results.
        Uses Reciprocal Rank Fusion (RRF) for ranking.
        """
        # 1. Vector Search
        vector_results = await self.search_memories(query_text, limit=limit * 2, agent_id=agent_id)
        
        # 2. Keyword Search (FTS5)
        keyword_results = []
        try:
            rows = await db_manager.fetchall(
                "SELECT id, content, metadata FROM knowledge_fts WHERE knowledge_fts MATCH ? LIMIT ?",
                query_text, limit * 2
            )
            for row in rows:
                keyword_results.append({
                    "id": row["id"],
                    "content": row["content"],
                    "metadata": json.loads(row["metadata"]),
                    "source": "keyword"
                })
        except Exception as e:
            logger.error(f"Keyword search failed: {e}")

        # 3. Reciprocal Rank Fusion (RRF)
        # Score = sum(1 / (k + rank))
        k = 60
        scores: Dict[str, float] = {}
        doc_map: Dict[str, Dict[str, Any]] = {}

        for rank, doc in enumerate(vector_results):
            doc_id = doc["id"]
            scores[doc_id] = scores.get(doc_id, 0) + 1.0 / (k + rank + 1)
            doc_map[doc_id] = doc

        for rank, doc in enumerate(keyword_results):
            doc_id = doc["id"]
            scores[doc_id] = scores.get(doc_id, 0) + 1.0 / (k + rank + 1)
            if doc_id not in doc_map:
                doc_map[doc_id] = doc

        # Sort by fused score
        fused_results = sorted(
            [{"score": score, **doc_map[doc_id]} for doc_id, score in scores.items()],
            key=lambda x: x["score"],
            reverse=True
        )

        return fused_results[:limit]

# --- Example Usage ---
async def main():
    """Demonstrates the usage of the DaaB class."""
    print("--- DaaB Demo (LanceDB) ---")
    daab_instance = DaaB()

    try:
        daab_instance.connect()

        # 1. Add memories
        await daab_instance.add_memory(
            "The user's favorite color is blue.",
            agent_id="demo_agent",
            metadata={"source": "conversation_1"},
        )
        await daab_instance.add_memory(
            "The project goal is to build a atlas.",
            agent_id="demo_agent",
            metadata={"source": "GEMINI.md"},
        )

        # 2. Search for a relevant memory
        print("\n--- Searching for 'What is the UI technology?' ---")
        search_results = await daab_instance.search_memories(
            "What is the UI technology?", agent_id="demo_agent"
        )
        for result in search_results:
            print(
                f"  - ID: {result['id']}, "
                f"Score: {result['score']:.4f}, "
                f"Content: {result['content']}"
            )

    except Exception as e:
        print(f"An error occurred during the demo: {e}")
    finally:
        daab_instance.close()
        print("\n--- DaaB Demo Finished ---")


if __name__ == "__main__":
    asyncio.run(main())
