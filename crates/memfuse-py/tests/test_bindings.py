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

def test_full_crud_and_metadata(tmp_path):
    db = memfuse.open(str(tmp_path / "test_crud"), dimension=4)
    col = db.collection("test")
    v1 = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    v2 = np.array([0.0, 1.0, 0.0, 0.0], dtype=np.float32)

    # Insert
    col.insert("d1", v1, metadata={"val": 1})

    # Get
    doc = col.get("d1")
    assert doc is not None
    assert doc.id == "d1"
    assert doc.metadata["val"] == 1

    # Update
    col.update("d1", v2, metadata={"val": 2})
    doc = col.get("d1")
    assert doc.metadata["val"] == 2

    # Search should now match v2 better
    res = col.search(v2, k=1)
    assert res[0].id == "d1"

    # Delete
    col.delete("d1")
    assert col.get("d1") is None
    assert len(col.search(v2, k=1)) == 0

def test_list_and_drop_collections(tmp_path):
    db = memfuse.open(str(tmp_path / "test_cols"), dimension=4)
    db.collection("c1")
    db.collection("c2")

    cols = db.list_collections()
    assert "default" in cols
    assert "c1" in cols
    assert "c2" in cols

    db.drop_collection("c1")
    cols = db.list_collections()
    assert "c1" not in cols
    assert "c2" in cols

def test_relationships_and_scanning(tmp_path):
    db = memfuse.open(str(tmp_path / "test_rel"), dimension=4)
    col = db.collection("graph")

    col.relate("alice", "bob", "follows")
    col.relate("alice", "charlie", "follows")

    # Scan prefix for alice's follows
    # In memfuse-db, relate internally uses __rel:{from}:{label}:{to} format
    # and namespaced_key(..., 2)
    rels = col.scan_prefix("__rel:alice:follows:")
    assert len(rels) == 2

    targets = [v["to"] for k, v in rels]
    assert "bob" in targets
    assert "charlie" in targets
