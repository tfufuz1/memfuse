import memfuse
import numpy as np
import pytest
import os
import shutil

@pytest.fixture
def db_path(tmp_path):
    path = str(tmp_path / "test_db")
    yield path
    if os.path.exists(path):
        shutil.rmtree(path)

def test_open_and_basic_insert_search(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("test")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    col.insert("doc1", v, metadata={"text": "hello world"})

    results = col.search(v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"
    assert results[0].metadata["text"] == "hello world"

def test_crud_operations(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("crud")
    v1 = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    v2 = np.array([0.0, 1.0, 0.0, 0.0], dtype=np.float32)

    # Create
    col.insert("k1", v1, metadata={"v": 1})
    doc = col.get("k1")
    assert doc is not None
    assert doc.id == "k1"
    assert doc.metadata["v"] == 1

    # Update
    col.update("k1", v2, metadata={"v": 2})
    doc = col.get("k1")
    assert doc.metadata["v"] == 2

    # Search should find updated vector
    results = col.search(v2, k=1)
    assert results[0].id == "k1"

    # Delete
    col.delete("k1")
    assert col.get("k1") is None
    assert len(col.search(v2, k=1)) == 0

def test_hybrid_search(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("docs")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    col.insert("doc1", v, metadata={"text": "rust programming language"})

    results = col.hybrid_search("rust programming", v, k=1)
    assert len(results) == 1
    assert results[0].id == "doc1"

def test_collection_management(db_path):
    db = memfuse.open(db_path, dimension=4)
    db.collection("col1")
    db.collection("col2")

    cols = db.list_collections()
    assert "col1" in cols
    assert "col2" in cols
    assert "default" in cols

    db.drop_collection("col1")
    cols = db.list_collections()
    assert "col1" not in cols
    assert "col2" in cols

def test_collection_isolation(db_path):
    db = memfuse.open(db_path, dimension=4)
    col_a = db.collection("a")
    col_b = db.collection("b")
    v = np.ones(4, dtype=np.float32)

    col_a.insert("k1", v)
    
    b_results = col_b.search(v, k=5)
    assert len(b_results) == 0
    assert col_b.get("k1") is None

def test_db_top_level_parity(db_path):
    db = memfuse.open(db_path, dimension=4)
    v = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)

    # Top-level CRUD
    db.insert("db_k1", v, metadata={"source": "top"})
    doc = db.get("db_k1")
    assert doc.id == "db_k1"
    assert doc.metadata["source"] == "top"

    results = db.search(v, k=1)
    assert results[0].id == "db_k1"

    assert db.len() == 1
    assert not db.is_empty()

    db.delete("db_k1")
    assert db.len() == 0

def test_relationships_and_scanning(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("rel")
    v = np.zeros(4, dtype=np.float32)

    col.insert("a", v)
    col.insert("b", v)
    col.relate("a", "b", "friend")

    # scan_prefix for relations
    # Note: internal prefixing might be visible or stripped depending on implementation
    # The Rust side for collection prepends __col:{name}:\x00
    # But scan_prefix in Collection handles it.
    rels = col.scan_prefix("__rel:a:friend:")
    assert len(rels) == 1
    assert rels[0][1]["to"] == "b"

    # db level scan_prefix (default collection)
    db.insert("x", v, metadata={"type": "agent"})
    db.insert("y", v, metadata={"type": "task"})

    # Scan all
    all_docs = db.scan()
    # db.scan() on default collection sees EVERYTHING because it has no prefix.
    # We just check that our keys are in there.
    doc_ids = [k for k, v in all_docs]
    assert "x" in doc_ids
    assert "y" in doc_ids

    # Named collection scan IS isolated
    rel_docs = col.scan()
    assert len(rel_docs) == 2
    rel_doc_ids = [k for k, v in rel_docs]
    assert "a" in rel_doc_ids
    assert "b" in rel_doc_ids

def test_statistics(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("stats_col")
    v = np.random.rand(4).astype(np.float32)
    col.insert("s1", v)

    c_stats = col.stats()
    assert c_stats.num_vectors == 1
    assert isinstance(c_stats.memory_usage_bytes, int)

    db_stats = db.stats()
    # db.stats() currently aggregates default collection
    assert db_stats.index_stats.num_vectors == 0 # because s1 is in "stats_col"
    assert db_stats.storage_stats.num_segments >= 0

def test_len_ergonomics(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("test_len")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)

    db.insert("d1", v)
    assert len(db) == 1

    col.insert("c1", v)
    assert len(col) == 1
    assert len(db) == 1 # db still 1 because c1 is in another collection

def test_open_custom_config(db_path):
    # This just ensures it doesn't crash and parameters are accepted
    db = memfuse.open(db_path, dimension=128, max_elements=5000, distance_metric="euclidean")
    assert db is not None

    # Check invalid metric
    with pytest.raises(ValueError):
        memfuse.open(db_path + "_invalid", distance_metric="invalid")
