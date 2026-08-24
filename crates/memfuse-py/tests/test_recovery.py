import memfuse
import numpy as np
import pytest
import os
import shutil

@pytest.fixture
def db_path(tmp_path):
    path = str(tmp_path / "recovery_db")
    yield path
    if os.path.exists(path):
        shutil.rmtree(path)

def test_crash_recovery_basic(db_path):
    # Simulate first process
    db1 = memfuse.open(db_path, dimension=4)
    col1 = db1.collection("recovery_col")
    
    v1 = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    v2 = np.array([0.0, 1.0, 0.0, 0.0], dtype=np.float32)
    
    col1.insert("doc1", v1, metadata={"val": 1})
    col1.insert("doc2", v2, metadata={"val": 2})
    
    # Simulate a "crash" by deleting the python objects without clean shutdown
    # (Though Rust drops will still run, this mimics restarting the process)
    del col1
    del db1
    
    # Second process opens the same DB
    db2 = memfuse.open(db_path, dimension=4)
    col2 = db2.collection("recovery_col")
    
    # Data should be recovered from WAL
    doc1 = col2.get("doc1")
    assert doc1 is not None
    assert doc1.metadata["val"] == 1
    
    doc2 = col2.get("doc2")
    assert doc2 is not None
    assert doc2.metadata["val"] == 2
    
    # Search should still work
    res = col2.search(v1, k=1)
    assert len(res) == 1
    assert res[0].id == "doc1"

def test_crash_recovery_with_deletes(db_path):
    db1 = memfuse.open(db_path, dimension=4)
    col1 = db1.collection("del_col")
    
    v = np.zeros(4, dtype=np.float32)
    col1.insert("doc_a", v)
    col1.insert("doc_b", v)
    col1.delete("doc_a")
    
    del col1
    del db1
    
    # Re-open
    db2 = memfuse.open(db_path, dimension=4)
    col2 = db2.collection("del_col")
    
    assert col2.get("doc_a") is None
    assert col2.get("doc_b") is not None
