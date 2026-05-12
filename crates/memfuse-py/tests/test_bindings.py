import memfuse
import numpy as np
import pytest

def test_open_and_basic_insert_search(tmp_path):
    db1 = memfuse.open(str(tmp_path / "test_wp31"), dimension=4)
    col = db1.collection("test")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    col.insert("doc1", v)
    results = col.search(v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"  # AC-1

def test_hybrid_search(tmp_path):
    db2 = memfuse.open(str(tmp_path / "test_wp31_hybrid"), dimension=4)
    col = db2.collection("docs")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    col.insert("doc1", v, metadata={"text": "rust programming language"})
    results = col.hybrid_search("rust programming", v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"  # AC-2

def test_collection_isolation(tmp_path):
    db3 = memfuse.open(str(tmp_path / "test_wp31_iso"), dimension=4)
    col_a = db3.collection("a")
    col_b = db3.collection("b")
    v = np.ones(4, dtype=np.float32)
    col_a.insert("k1", v)
    
    b_results = col_b.search(v, k=5)
    assert len(b_results) == 0  # AC-3
