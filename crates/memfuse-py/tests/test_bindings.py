import memfuse
import numpy as np
import pytest

def test_open_and_basic_insert_search(tmp_path):
    # Old open API (backward compat)
    db1 = memfuse.open(str(tmp_path / "test_wp31"), dimension=4)
    col = db1.collection("test")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    col.insert("doc1", v, metadata={"tags": ["rust", "test"]})

    results = col.search(v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"
    assert results[0].score > 0.99
    assert results[0].metadata == {"tags": ["rust", "test"]}

def test_open_with_config(tmp_path):
    config = memfuse.MemFuseConfig(dimension=4, distance_metric=memfuse.DistanceMetric.Euclidean)
    db = memfuse.MemFuse.open_with_config(str(tmp_path / "test_config"), config)
    col = db.collection("test")
    v = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    col.insert("d1", v)

    # Euclidean distance of 0 for exact match maps to score 1.0
    results = col.search(v, k=1)
    assert len(results) == 1
    assert results[0].score > 0.9999

def test_hybrid_search(tmp_path):
    db2 = memfuse.open(str(tmp_path / "test_wp31_hybrid"), dimension=4)
    col = db2.collection("docs")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    col.insert("doc1", v, metadata={"text": "rust programming language"})
    results = col.hybrid_search("rust programming", v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"

def test_collection_isolation(tmp_path):
    db3 = memfuse.open(str(tmp_path / "test_wp31_iso"), dimension=4)
    col_a = db3.collection("a")
    col_b = db3.collection("b")
    v = np.ones(4, dtype=np.float32)
    col_a.insert("k1", v)
    
    b_results = col_b.search(v, k=5)
    assert len(b_results) == 0

def test_get_update_delete(tmp_path):
    db = memfuse.open(str(tmp_path / "test_crud"), dimension=4)
    col = db.collection("test")
    v = np.array([1,0,0,0], dtype=np.float32)

    col.insert("k1", v, {"v": 1})
    doc = col.get("k1")
    assert doc["id"] == "k1"
    assert doc["metadata"]["v"] == 1

    col.update("k1", v, {"v": 2})
    doc = col.get("k1")
    assert doc["metadata"]["v"] == 2

    col.delete("k1")
    assert col.get("k1") is None
    assert col.len() == 0

def test_relate_and_scan(tmp_path):
    db = memfuse.open(str(tmp_path / "test_rel"), dimension=4)
    col = db.collection("test")
    v = np.zeros(4, dtype=np.float32)
    col.insert("a", v)
    col.insert("b", v)

    col.relate("a", "b", "friend")

    res = col.scan_prefix("__rel:a:friend:")
    assert len(res) == 1
    assert res[0][1]["to"] == "b"

def test_list_drop_collections(tmp_path):
    db = memfuse.open(str(tmp_path / "test_cols"), dimension=4)
    db.collection("c1")
    db.collection("c2")

    cols = db.list_collections()
    assert "c1" in cols
    assert "c2" in cols

    db.drop_collection("c1")
    cols = db.list_collections()
    assert "c1" not in cols
    assert "c2" in cols

def test_stats(tmp_path):
    db = memfuse.open(str(tmp_path / "test_stats"), dimension=4)
    col = db.collection("test")
    col.insert("k1", np.zeros(4, dtype=np.float32))

    s = col.stats()
    assert s["num_vectors"] == 1
    assert "memory_usage_bytes" in s
