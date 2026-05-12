import memfuse
import numpy as np
import pytest

def test_open_and_basic_insert_search(tmp_path):
    db = memfuse.open(str(tmp_path / "test_wp31"), dimension=4)
    col = db.collection("test")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    col.insert("doc1", v)
    results = col.search(v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"
    # Test dictionary-like access
    assert results[0]["id"] == "doc1"
    assert results[0]["score"] >= 0.0

def test_hybrid_search(tmp_path):
    db = memfuse.open(str(tmp_path / "test_wp31_hybrid"), dimension=4)
    col = db.collection("docs")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    col.insert("doc1", v, metadata={"text": "rust programming language"})
    results = col.hybrid_search("rust programming", v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"

def test_collection_isolation(tmp_path):
    db = memfuse.open(str(tmp_path / "test_wp31_iso"), dimension=4)
    col_a = db.collection("a")
    col_b = db.collection("b")
    v = np.ones(4, dtype=np.float32)
    col_a.insert("k1", v)
    
    b_results = col_b.search(v, k=5)
    assert len(b_results) == 0

def test_crud_and_relations(tmp_path):
    db = memfuse.open(str(tmp_path / "test_crud"), dimension=4)
    v1 = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    v2 = np.array([0.0, 1.0, 0.0, 0.0], dtype=np.float32)

    db.insert("d1", v1, metadata={"val": 1})
    db.insert("d2", v2, metadata={"val": 2})

    doc = db.get("d1")
    assert doc is not None
    assert doc.id == "d1"
    assert doc.metadata["val"] == 1

    db.relate("d1", "d2", "connected")
    # Relation check would need scan_prefix which we didn't expose yet to top-level Db,
    # but we can check if it doesn't crash.

    db.delete("d1")
    assert db.get("d1") is None

def test_list_and_drop_collections(tmp_path):
    db = memfuse.open(str(tmp_path / "test_cols"), dimension=4)
    db.collection("c1")
    db.collection("c2")

    cols = db.list_collections()
    assert "c1" in cols
    assert "c2" in cols
    assert "default" in cols

    db.drop_collection("c1")
    cols = db.list_collections()
    assert "c1" not in cols
    assert "c2" in cols
