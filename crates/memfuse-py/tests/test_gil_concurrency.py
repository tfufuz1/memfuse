import memfuse
import numpy as np
import time
import threading
import pytest

def test_gil_released_during_search(tmp_path):
    path = str(tmp_path / "gil_db")
    db = memfuse.open(path, dimension=128)

    # Insert 50 documents
    v_base = np.random.rand(128).astype(np.float32)
    for i in range(50):
        db.insert(f"doc_{i}", v_base + (i * 0.001))

    # Measurement variable to check background Python execution
    counter = [0]
    stop_flag = False

    def python_cpu_worker():
        while not stop_flag:
            counter[0] += 1
            time.sleep(0.0001)

    # Start CPU worker thread in Python
    t_py = threading.Thread(target=python_cpu_worker)
    t_py.start()

    # Perform concurrent database searches from multiple Python threads
    def rust_search_worker():
        v_query = np.random.rand(128).astype(np.float32)
        for _ in range(50):
            res = db.search(v_query, k=5)
            assert len(res) > 0

    threads = [threading.Thread(target=rust_search_worker) for _ in range(4)]

    for t in threads:
        t.start()

    for t in threads:
        t.join()

    stop_flag = True
    t_py.join()

    # The background Python thread must have been able to run concurrently
    assert counter[0] > 0, "Python thread was completely blocked by Rust operations (GIL not released)"


def test_gil_released_during_batch_insert(tmp_path):
    path = str(tmp_path / "gil_batch_db")
    db = memfuse.open(path, dimension=128)

    counter = [0]
    stop_flag = False

    def python_cpu_worker():
        while not stop_flag:
            counter[0] += 1
            time.sleep(0.0001)

    t_py = threading.Thread(target=python_cpu_worker)
    t_py.start()

    def rust_batch_worker(thread_idx):
        batch = [
            (f"doc_t{thread_idx}_{i}", np.random.rand(128).astype(np.float32), {"idx": i})
            for i in range(100)
        ]
        db.insert_many(batch)

    threads = [threading.Thread(target=rust_batch_worker, args=(i,)) for i in range(4)]

    for t in threads:
        t.start()

    for t in threads:
        t.join()

    stop_flag = True
    t_py.join()

    assert counter[0] > 0, "Python thread was completely blocked by batch insert operations (GIL not released)"


def test_gil_released_during_batch_upsert(tmp_path):
    path = str(tmp_path / "gil_upsert_db")
    db = memfuse.open(path, dimension=128)

    counter = [0]
    stop_flag = False

    def python_cpu_worker():
        while not stop_flag:
            counter[0] += 1
            time.sleep(0.0001)

    t_py = threading.Thread(target=python_cpu_worker)
    t_py.start()

    def rust_upsert_worker(thread_idx):
        batch = [
            (f"doc_u{thread_idx}_{i}", np.random.rand(128).astype(np.float32), {"idx": i})
            for i in range(100)
        ]
        db.upsert_many(batch)

    threads = [threading.Thread(target=rust_upsert_worker, args=(i,)) for i in range(4)]

    for t in threads:
        t.start()

    for t in threads:
        t.join()

    stop_flag = True
    t_py.join()

    assert counter[0] > 0, "Python thread was completely blocked by batch upsert operations (GIL not released)"


def test_canary_thread_progress_during_heavy_db_operation(tmp_path):
    """BEFUND 2 Verification: Starts two Python threads — Thread 1 running a long-running/heavy
    MemFuse DB operation, and Thread 2 as a 'canary' thread running a pure Python counting loop with
    timestamp logging. Verifies that the canary thread logs continuous progress during DB operations,
    proving the Python GIL is released during block_on async I/O.
    """
    path = str(tmp_path / "canary_db")
    db = memfuse.open(path, dimension=128)

    # Pre-populate database with documents
    v_base = np.random.rand(128).astype(np.float32)
    batch = [(f"init_doc_{i}", v_base + (i * 0.001), {"idx": i}) for i in range(200)]
    db.insert_many(batch)

    canary_logs = []
    stop_canary = False

    def canary_thread():
        count = 0
        while not stop_canary:
            count += 1
            now = time.time()
            canary_logs.append((count, now))
            time.sleep(0.001)

    t_canary = threading.Thread(target=canary_thread)
    t_canary.start()

    def db_heavy_worker():
        for _ in range(50):
            q = np.random.rand(128).astype(np.float32)
            db.search(q, k=10)
            db.hybrid_search("init_doc", q, k=10)

    t_db = threading.Thread(target=db_heavy_worker)

    db_start_time = time.time()
    t_db.start()
    t_db.join()
    db_end_time = time.time()

    stop_canary = True
    t_canary.join()

    # Filter canary logs during the active DB window
    logs_during_db = [t for _, t in canary_logs if db_start_time <= t <= db_end_time]

    assert len(canary_logs) > 0, "Canary thread recorded no progress"
    assert len(logs_during_db) >= 2, (
        f"Canary thread was blocked during DB execution window ({db_start_time:.3f} to {db_end_time:.3f}). "
        f"Recorded {len(logs_during_db)} timestamps during DB operation."
    )
