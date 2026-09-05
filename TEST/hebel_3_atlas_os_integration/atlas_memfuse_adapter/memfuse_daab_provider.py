"""
MemFuse DaaB Provider for Atlas OS.

Drop-in replacement for `atlas/apps/kernel/src_agents/daab/core.py`.
Directly interfaces with `memfuse-py` (Rust PyO3 engine) to provide
unified 4-signal hybrid retrieval, MVCC transactional storage,
and long-term memory for Atlas LangGraph agents.
"""

import asyncio
import json
import logging
import os
import time
import uuid
from pathlib import Path
from typing import Any, Dict, List, Optional

logger = logging.getLogger("atlas.memfuse_daab")

try:
    import memfuse
    HAS_MEMFUSE = True
except ImportError:
    HAS_MEMFUSE = False
    logger.warning("memfuse-py not found in Python path; using mock engine for standalone validation.")


class MemFuseDaaBProvider:
    """
    Universal L4 Memory Engine for Atlas OS, backed by MemFuse.
    Eliminates the fragmented SQLite + LanceDB stack.
    """

    def __init__(self, db_path: Optional[str] = None):
        if db_path:
            self.db_path = db_path
        else:
            self.db_path = os.getenv(
                "MEMFUSE_PATH",
                str(Path.home() / ".atlas" / "data" / "memfuse_store")
            )
        self._db = None
        self._collection = None
        self._collection_name = "atlas_agent_memory"
        self._connected = False
        logger.info(f"Initialized MemFuseDaaBProvider with storage path: {self.db_path}")

    async def connect(self):
        """Initializes the MemFuse database and default collection."""
        if self._connected:
            return

        os.makedirs(self.db_path, exist_ok=True)

        if HAS_MEMFUSE:
            # Native Rust PyO3 initialization
            self._db = memfuse.Database(self.db_path)
            self._collection = self._db.get_or_create_collection(self._collection_name)
            logger.info("Connected to native MemFuse Rust engine via PyO3.")
        else:
            # In-memory mock for local pipeline verification
            self._mock_memories: Dict[str, Dict[str, Any]] = {}
            logger.info("Connected to Mock MemFuse engine.")

        self._connected = True

    async def add_memory(
        self,
        content: str,
        agent_id: str,
        metadata: Optional[Dict[str, Any]] = None,
        importance_score: float = 1.0,
    ) -> str:
        """
        Stores an agent interaction or memory item into MemFuse.
        Automatically embeds content, builds BM25 index, and creates graph node.
        """
        if not self._connected:
            await self.connect()

        memory_id = str(uuid.uuid4())
        meta = metadata.copy() if metadata else {}
        meta.update({
            "agent_id": agent_id,
            "created_at": time.time(),
            "importance": importance_score,
        })

        if HAS_MEMFUSE:
            # memfuse handles text embedding, keyword indexing, and metadata atomicity
            self._collection.insert(
                id=memory_id,
                text=content,
                metadata=meta,
            )
        else:
            self._mock_memories[memory_id] = {
                "id": memory_id,
                "content": content,
                "agent_id": agent_id,
                "metadata": meta,
                "created_at": time.time(),
            }

        logger.debug(f"Added memory {memory_id} for agent {agent_id}")
        return memory_id

    async def search_memories(
        self,
        query: str,
        agent_id: Optional[str] = None,
        limit: int = 5,
        min_score: float = 0.0,
    ) -> List[Dict[str, Any]]:
        """
        Executes 4-signal hybrid retrieval (Vector + BM25 + Graph + Metadata Filter)
        fused via Reciprocal Rank Fusion (RRF k=60).
        """
        if not self._connected:
            await self.connect()

        if HAS_MEMFUSE:
            filter_expr = f"agent_id == '{agent_id}'" if agent_id else None
            results = self._collection.hybrid_search(
                query=query,
                limit=limit,
                filter=filter_expr,
            )
            formatted = []
            for r in results:
                formatted.append({
                    "id": r.id,
                    "content": r.text,
                    "score": r.score,
                    "metadata": r.metadata,
                })
            return formatted
        else:
            # Simple lexical filter for mock
            results = []
            q_lower = query.lower()
            for m in self._mock_memories.values():
                if agent_id and m["agent_id"] != agent_id:
                    continue
                score = 0.8 if q_lower in m["content"].lower() else 0.2
                if score >= min_score:
                    results.append({
                        "id": m["id"],
                        "content": m["content"],
                        "score": score,
                        "metadata": m["metadata"],
                    })
            results.sort(key=lambda x: x["score"], reverse=True)
            return results[:limit]

    async def delete_memory(self, memory_id: str) -> bool:
        """Deletes a memory item by ID."""
        if not self._connected:
            await self.connect()

        if HAS_MEMFUSE:
            return self._collection.delete(memory_id)
        else:
            return self._mock_memories.pop(memory_id, None) is not None
