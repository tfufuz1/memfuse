import memfuse
import numpy as np
import pytest
import os

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
    assert results[0].metadata["text"] == "rust programming language"

def test_collection_isolation(tmp_path):
    db3 = memfuse.open(str(tmp_path / "test_wp31_iso"), dimension=4)
    col_a = db3.collection("a")
    col_b = db3.collection("b")
    v = np.ones(4, dtype=np.float32)
    col_a.insert("k1", v)
    
    b_results = col_b.search(v, k=5)
    assert len(b_results) == 0  # AC-3

def test_get_delete_update(tmp_path):
    db = memfuse.open(str(tmp_path / "test_gdu"), dimension=4)
    v = np.array([1, 0, 0, 0], dtype=np.float32)
    db.insert("k1", v, metadata={"v": 1})

    doc = db.get("k1")
    assert doc["id"] == "k1"
    assert doc["metadata"]["v"] == 1

    db.update("k1", v, metadata={"v": 2})
    doc = db.get("k1")
    assert doc["metadata"]["v"] == 2

    db.delete("k1")
    assert db.get("k1") is None

def test_list_and_drop_collections(tmp_path):
    db = memfuse.open(str(tmp_path / "test_coll_mgmt"), dimension=4)
    db.collection("c1")
    db.collection("c2")

    colls = db.list_collections()
    assert "c1" in colls
    assert "c2" in colls
    assert "default" in colls

    db.drop_collection("c1")
    colls = db.list_collections()
    assert "c1" not in colls
    assert "c2" in colls

def test_stats_and_len(tmp_path):
    db = memfuse.open(str(tmp_path / "test_stats"), dimension=4)
    v = np.zeros(4, dtype=np.float32)
    db.insert("k1", v)
    db.insert("k2", v)

    assert db.len() == 2
    assert not db.is_empty()

    stats = db.stats()
    assert stats["index"]["num_vectors"] == 2
    assert "storage" in stats

def test_relate_and_scan(tmp_path):
    db = memfuse.open(str(tmp_path / "test_relate"), dimension=4)
    v = np.zeros(4, dtype=np.float32)
    db.insert("a", v, metadata={"name": "Alice"})
    db.insert("b", v, metadata={"name": "Bob"})

    db.relate("a", "b", "friend")

    # scan_prefix is tricky because of internal prefixing, but top-level should work for default
    results = db.scan_prefix("__rel:a:friend:")
    assert len(results) == 1
    assert results[0]["value"]["to"] == "b"
