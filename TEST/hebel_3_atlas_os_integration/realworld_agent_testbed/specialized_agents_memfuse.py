"""
Specialized Agents Memory Testbed for Atlas OS + MemFuse.

Adapts Atlas specialized agent workflows (from `apps/kernel/src_agents/specialized_agents.py`)
to use MemFuse for agent state persistence, context compaction, and RAG retrieval.
"""

import asyncio
import logging
import time
from typing import Any, Dict, List, Optional

from ..atlas_memfuse_adapter.memfuse_daab_provider import MemFuseDaaBProvider

logger = logging.getLogger("atlas.specialized_agents_memfuse")


class MemFuseAgentHarness:
    """
    Test harness for simulating Atlas LangGraph specialized agents
    operating on top of MemFuse memory.
    """

    def __init__(self, provider: MemFuseDaaBProvider):
        self.provider = provider

    async def execute_agent_step(
        self,
        agent_id: str,
        task_prompt: str,
        observations: List[str],
    ) -> Dict[str, Any]:
        """
        Simulates one full step of an Atlas Agent:
        1. Context retrieval from MemFuse (4-signal hybrid search)
        2. Thought & tool execution
        3. Decision persistence into MemFuse
        """
        start_time = time.perf_counter()

        # Step 1: Retrieve context
        retrieved_context = await self.provider.search_memories(
            query=task_prompt,
            agent_id=agent_id,
            limit=3,
        )

        # Step 2: Simulate thought / action
        action_result = f"Agent {agent_id} completed task '{task_prompt[:30]}...' with {len(observations)} observations."

        # Step 3: Persist decision & findings into MemFuse
        memory_id = await self.provider.add_memory(
            content=f"Decision by {agent_id}: {action_result}",
            agent_id=agent_id,
            metadata={
                "task": task_prompt,
                "context_hits": len(retrieved_context),
                "duration_ms": (time.perf_counter() - start_time) * 1000.0,
            },
        )

        return {
            "agent_id": agent_id,
            "memory_id": memory_id,
            "context_count": len(retrieved_context),
            "duration_ms": (time.perf_counter() - start_time) * 1000.0,
            "status": "SUCCESS",
        }
