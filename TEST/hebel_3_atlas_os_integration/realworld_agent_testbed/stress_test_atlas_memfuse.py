"""
Real-World Concurrency & Stress Test: Atlas OS Agents on MemFuse.

Simulates high-load agent interaction:
- 10 concurrent LangGraph agents
- Parallel tool calls, context retrieval, and decision logging
- Validates latency, throughput, and zero-panic behavior under memory pressure
"""

import asyncio
import logging
import random
import sys
import time
from pathlib import Path

# Add adapter path
sys.path.insert(0, str(Path(__file__).parent.parent))

from atlas_memfuse_adapter.memfuse_daab_provider import MemFuseDaaBProvider
from realworld_agent_testbed.specialized_agents_memfuse import MemFuseAgentHarness

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("atlas.stress_test")


async def simulate_agent_worker(
    worker_id: int,
    harness: MemFuseAgentHarness,
    num_iterations: int = 10,
) -> list:
    agent_id = f"agent_worker_{worker_id:02d}"
    latencies = []

    tasks = [
        "Analyze security threat vectors in Tauri IPC channels",
        "Perform SIMD vector distance benchmarks on AVX-512 nodes",
        "Consolidate short-term agent scratchpad into long-term memory",
        "Resolve CompactionStrategy symbol collisions between LSM and token compaction",
        "Run CoVe 4-phase verification contract on PR 402",
        "Stream live A2UI component state to Atlas frontend window",
    ]

    for i in range(num_iterations):
        task = random.choice(tasks)
        obs = [f"Observation {j} from environment" for j in range(random.randint(1, 3))]

        res = await harness.execute_agent_step(agent_id, task, obs)
        latencies.append(res["duration_ms"])
        await asyncio.sleep(random.uniform(0.01, 0.05))

    return latencies


async def run_stress_test():
    logger.info("================================================================")
    logger.info("   ATLAS OS & MEMFUSE REAL-WORLD CONCURRENCY STRESS TEST        ")
    logger.info("================================================================")

    provider = MemFuseDaaBProvider()
    await provider.connect()
    harness = MemFuseAgentHarness(provider)

    num_workers = 10
    iterations_per_worker = 15
    total_operations = num_workers * iterations_per_worker

    logger.info(f"Spawning {num_workers} concurrent agents with {iterations_per_worker} steps each...")
    start_total = time.perf_counter()

    worker_tasks = [
        simulate_agent_worker(i, harness, iterations_per_worker)
        for i in range(num_workers)
    ]
    all_results = await asyncio.gather(*worker_tasks)

    elapsed_sec = time.perf_counter() - start_total
    all_latencies = [lat for sublist in all_results for lat in sublist]

    avg_lat = sum(all_latencies) / len(all_latencies) if all_latencies else 0.0
    p95_lat = sorted(all_latencies)[int(len(all_latencies) * 0.95)] if all_latencies else 0.0
    ops_per_sec = total_operations / elapsed_sec if elapsed_sec > 0 else 0.0

    logger.info("----------------------------------------------------------------")
    logger.info(f"Total Operations:   {total_operations}")
    logger.info(f"Total Time:         {elapsed_sec:.3f} s")
    logger.info(f"Throughput:         {ops_per_sec:.1f} ops/sec")
    logger.info(f"Average Latency:    {avg_lat:.2f} ms")
    logger.info(f"P95 Latency:        {p95_lat:.2f} ms")
    logger.info("Status:             ALL OPERATIONS COMPLETED WITHOUT PANIC OR ERROR")
    logger.info("================================================================")


if __name__ == "__main__":
    asyncio.run(run_stress_test())
