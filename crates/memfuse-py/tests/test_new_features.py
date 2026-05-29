import numpy as np
import pytest
import tempfile
import shutil
import os
import memfuse

@pytest.fixture
def db_path():
    path = tempfile.mkdtemp()
    yield path
    if os.path.exists(path):
        shutil.rmtree(path)

def test_db_open_close(db_path):
    db = memfuse.open(db_path, dimension=4)
    db.insert("doc1", np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32), {"key": "val"})
    assert len(db) == 1
    db.flush()
    # Implicitly closed by scope if we had a close method, but flush ensures persistence

def test_metadata_filter(db_path):
    db = memfuse.open(db_path, dimension=4)
    db.insert("doc1", np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32), {"category": "A", "val": 10})
    db.insert("doc2", np.array([0.0, 1.0, 0.0, 0.0], dtype=np.float32), {"category": "B", "val": 20})

    # Search with filter
    results = db.search(np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32), k=10, filter={"category": "A"})
    assert len(results) == 1
    assert results[0].id == "doc1"

    results = db.search(np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32), k=10, filter={"category": "B"})
    assert len(results) == 1
    assert results[0].id == "doc2"

    results = db.search(np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32), k=10, filter={"nonexistent": "match"})
    assert len(results) == 0

def test_repair_method(db_path):
    db = memfuse.open(db_path, dimension=4)
    # Just call it to ensure it doesn't crash and is exposed
    db.repair()
    col = db.collection("test")
    col.repair()

def test_hybrid_search_filter_error(db_path):
    db = memfuse.open(db_path, dimension=4)
    with pytest.raises(RuntimeError) as excinfo:
        db.hybrid_search("test", np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32), k=10, filter={"key": "val"})
    assert "Metadata filtering is not yet supported for hybrid search" in str(excinfo.value)
