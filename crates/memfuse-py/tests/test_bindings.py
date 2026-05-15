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

def test_relate_and_scan(db_path):
    db = memfuse.open(db_path, dimension=4)
    v = np.zeros(4, dtype=np.float32)
    db.insert("doc1", v)
    db.insert("doc2", v)

    db.relate("doc1", "doc2", "knows")

    # scan_prefix for relations
    rels = db.scan_prefix("__rel:doc1:knows:")
    assert len(rels) == 1
    assert rels[0][1]["to"] == "doc2"

    # Backward relation should also exist because db.relate in Rust is bidirectional
    rels_bwd = db.scan_prefix("__rel:doc2:knows:")
    assert len(rels_bwd) == 1
    assert rels_bwd[0][1]["to"] == "doc1"

def test_stats_and_len(db_path):
    db = memfuse.open(db_path, dimension=4)
    assert db.is_empty()
    assert db.len() == 0

    v = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    db.insert("doc1", v)

    assert not db.is_empty()
    assert db.len() == 1

    stats = db.stats()
    assert stats.index_stats.num_vectors == 1
    assert stats.storage_stats.num_segments >= 0 # Initial might be 0 until flush

def test_encryption_config(db_path):
    # Just verify it opens with encryption config
    db = memfuse.open(db_path, dimension=4, encryption_passphrase="secret-stuff")
    v = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    db.insert("doc1", v)
    assert db.len() == 1

def test_distance_metric_config(db_path):
    db = memfuse.open(db_path, dimension=4, distance_metric="euclidean")
    v1 = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    v2 = np.array([1.1, 0.0, 0.0, 0.0], dtype=np.float32)
    db.insert("doc1", v1)

    results = db.search(v2, k=1)
    assert results[0].id == "doc1"
    # Euclidean score is 1 / (1 + dist). dist is 0.1, score should be ~0.909
    assert 0.90 < results[0].score < 0.92
