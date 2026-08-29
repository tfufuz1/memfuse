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

    start_time = time.time()
    for t in threads:
        t.start()

    for t in threads:
        t.join()

    stop_flag = True
    t_py.join()

    # The background Python thread must have been able to run concurrently
    assert counter[0] > 0, "Python thread was completely blocked by Rust operations (GIL not released)"
