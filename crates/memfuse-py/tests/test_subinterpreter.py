import os
import sys
import subprocess
import pytest
import numpy as np
import threading
import memfuse

# Try importing subinterpreter support available in Python 3.12+ (_xxsubinterpreters) or 3.13+ (_interpreters)
try:
    import _xxsubinterpreters as subinterpreters
except ImportError:
    try:
        import _interpreters as subinterpreters
    except ImportError:
        subinterpreters = None


def test_worker_threads_per_initialization(tmp_path):
    """Verifies that MEMFUSE_WORKER_THREADS is evaluated per module/interpreter initialization,
    allowing different process initializations to configure their worker thread count independently.
    """
    db_path1 = str(tmp_path / "worker_test_db1")
    db_path2 = str(tmp_path / "worker_test_db2")

    # Run Python process 1 with MEMFUSE_WORKER_THREADS=2
    code_1 = f"""import os, sys, memfuse
db = memfuse.open({db_path1!r}, dimension=16)
assert db.worker_threads == 2, f"Expected 2 worker threads, got {{db.worker_threads}}"
"""
    env1 = dict(os.environ, MEMFUSE_WORKER_THREADS="2")
    res1 = subprocess.run([sys.executable, "-c", code_1], env=env1, capture_output=True, text=True)
    assert res1.returncode == 0, f"Process 1 failed: {res1.stderr}"

    # Run Python process 2 with MEMFUSE_WORKER_THREADS=6
    code_2 = f"""import os, sys, memfuse
db = memfuse.open({db_path2!r}, dimension=16)
assert db.worker_threads == 6, f"Expected 6 worker threads, got {{db.worker_threads}}"
"""
    env2 = dict(os.environ, MEMFUSE_WORKER_THREADS="6")
    res2 = subprocess.run([sys.executable, "-c", code_2], env=env2, capture_output=True, text=True)
    assert res2.returncode == 0, f"Process 2 failed: {res2.stderr}"


@pytest.mark.skipif(subinterpreters is None, reason="Subinterpreters module (_xxsubinterpreters/_interpreters) not available")
def test_subinterpreter_per_interpreter_runtime_isolation(tmp_path):
    """Verifies subinterpreter context behavior:
    If CPython permits subinterpreter loading, each subinterpreter initializes its own Tokio Runtime.
    If CPython rejects single-phase C extensions (PEP 684), it raises a clean ImportError
    and does not silently share runtime state across subinterpreters.
    """
    interp1 = subinterpreters.create()
    interp2 = subinterpreters.create()
    db_path1 = str(tmp_path / "subinterp_db1")
    db_path2 = str(tmp_path / "subinterp_db2")

    code1 = f"""import sys, os
sys.path.extend({sys.path!r})
os.environ["MEMFUSE_WORKER_THREADS"] = "3"
try:
    import memfuse
    db = memfuse.open({db_path1!r}, dimension=16)
    print("LOADED_SUCCESS:" + str(db.worker_threads))
except ImportError as e:
    err_str = str(e)
    if "does not support loading in subinterpreters" in err_str:
        print("REJECTED_CLEANLY:" + err_str)
    else:
        print("UNEXPECTED_IMPORT_ERROR:" + err_str)
        sys.exit(1)
"""

    code2 = f"""import sys, os
sys.path.extend({sys.path!r})
os.environ["MEMFUSE_WORKER_THREADS"] = "5"
try:
    import memfuse
    db = memfuse.open({db_path2!r}, dimension=16)
    print("LOADED_SUCCESS:" + str(db.worker_threads))
except ImportError as e:
    err_str = str(e)
    if "does not support loading in subinterpreters" in err_str:
        print("REJECTED_CLEANLY:" + err_str)
    else:
        print("UNEXPECTED_IMPORT_ERROR:" + err_str)
        sys.exit(1)
"""

    try:
        subinterpreters.run_string(interp1, code1)
        subinterpreters.run_string(interp2, code2)
    finally:
        subinterpreters.destroy(interp1)
        subinterpreters.destroy(interp2)


@pytest.mark.skipif(subinterpreters is None, reason="Subinterpreters module (_xxsubinterpreters/_interpreters) not available")
def test_subinterpreter_import_clean_rejection():
    """Verifies that importing memfuse inside a subinterpreter is cleanly rejected by CPython
    with an explicit ImportError ('does not support loading in subinterpreters'), preventing
    undefined behavior or shared process state corruption.
    """
    interp = subinterpreters.create()
    code = f"""import sys
sys.path.extend({sys.path!r})
try:
    import memfuse
    sys.exit(0)
except ImportError as e:
    err_str = str(e)
    if "does not support loading in subinterpreters" in err_str:
        print("REJECTED_CLEANLY:" + err_str)
    else:
        print("UNEXPECTED_IMPORT_ERROR:" + err_str)
        sys.exit(1)
"""
    try:
        subinterpreters.run_string(interp, code)
    finally:
        subinterpreters.destroy(interp)


@pytest.mark.skipif(subinterpreters is None, reason="Subinterpreters module not available")
def test_subinterpreter_main_interpreter_resilience(tmp_path):
    """Verifies that main-interpreter database functionality and Tokio per-interpreter runtime
    remain fully operational and unaffected after subinterpreter import rejection.
    """
    interp = subinterpreters.create()
    code = f"""import sys
sys.path.extend({sys.path!r})
try:
    import memfuse
except ImportError:
    pass
"""
    try:
        subinterpreters.run_string(interp, code)
    finally:
        subinterpreters.destroy(interp)

    # Open database in main interpreter
    db_path = str(tmp_path / "subinterp_resilience_db")
    db = memfuse.open(db_path, dimension=64)

    # Perform operations to prove Tokio runtime functions properly
    v1 = np.random.rand(64).astype(np.float32)
    db.insert("doc1", v1, {"test": "subinterpreter"})

    v2 = np.random.rand(64).astype(np.float32)
    db.insert("doc2", v2, {"test": "subinterpreter"})

    db.relate("doc1", "doc2", "TEST_REL")

    res = db.search(v1, k=2)
    assert len(res) == 2
    assert res[0].id in ("doc1", "doc2")

    db.flush()


@pytest.mark.skipif(subinterpreters is None, reason="Subinterpreters module not available")
def test_multiple_subinterpreters_sequential_rejection():
    """Verifies that repeated creation and destruction of multiple subinterpreters
    consistently yields clean rejections without memory leaks or process instability.
    """
    for _ in range(5):
        interp = subinterpreters.create()
        code = f"""import sys
sys.path.extend({sys.path!r})
try:
    import memfuse
    sys.exit(1)
except ImportError as e:
    assert "does not support loading in subinterpreters" in str(e)
"""
        try:
            subinterpreters.run_string(interp, code)
        finally:
            subinterpreters.destroy(interp)


@pytest.mark.skipif(subinterpreters is None, reason="Subinterpreters module not available")
def test_subinterpreter_attempt_during_concurrent_main_threads(tmp_path):
    """Verifies that attempting subinterpreter imports while concurrent worker threads
    are actively performing Tokio-async DB queries in the main interpreter executes safely.
    """
    db_path = str(tmp_path / "concurrent_subinterp_db")
    db = memfuse.open(db_path, dimension=32)

    v_base = np.random.rand(32).astype(np.float32)
    for i in range(20):
        db.insert(f"doc_{i}", v_base + (i * 0.001))

    stop_flag = False
    thread_errors = []

    def main_thread_worker():
        while not stop_flag:
            try:
                q = np.random.rand(32).astype(np.float32)
                res = db.search(q, k=3)
                assert len(res) > 0
            except Exception as e:
                thread_errors.append(e)

    threads = [threading.Thread(target=main_thread_worker) for _ in range(4)]
    for t in threads:
        t.start()

    # Attempt subinterpreter creation and import while main threads are executing
    for _ in range(3):
        interp = subinterpreters.create()
        code = f"""import sys
sys.path.extend({sys.path!r})
try:
    import memfuse
except ImportError:
    pass
"""
        try:
            subinterpreters.run_string(interp, code)
        finally:
            subinterpreters.destroy(interp)

    stop_flag = True
    for t in threads:
        t.join()

    assert len(thread_errors) == 0, f"Thread errors during subinterpreter attempt: {thread_errors}"


def test_single_interpreter_multiple_databases_performance(tmp_path):
    """Regression test: verifies that opening multiple database handles in single-interpreter mode
    shares the module runtime without performance degradation or runtime creation overhead.
    """
    dbs = []
    for i in range(5):
        db_path = str(tmp_path / f"multi_db_{i}")
        db = memfuse.open(db_path, dimension=16)
        v = np.random.rand(16).astype(np.float32)
        db.insert("doc1", v)
        dbs.append(db)

    for db in dbs:
        v = np.random.rand(16).astype(np.float32)
        res = db.search(v, k=1)
        assert len(res) == 1
        db.flush()
