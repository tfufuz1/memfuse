import memfuse
import numpy as np
import pytest
import os
import shutil

@pytest.fixture
def db_path(tmp_path):
    path = str(tmp_path / "refinement_db")
    yield path
    if os.path.exists(path):
        shutil.rmtree(path)

def test_repr_outputs(db_path):
    db = memfuse.open(db_path, dimension=4)
    v = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    db.insert("doc1", v, metadata={"key": "val"})

    # Test PyDocument repr
    doc = db.get("doc1")
    assert repr(doc) == "Document(id='doc1', metadata={'key': 'val'})"

    # Test PySearchResult repr
    results = db.search(v, k=1)
    res = results[0]
    # score might vary slightly, so we check start and end
    r_str = repr(res)
    assert r_str.startswith("SearchResult(id='doc1', score=")
    assert r_str.endswith(", metadata={'key': 'val'})")

    # Test Stats repr
    stats = db.stats()
    s_str = repr(stats)
    assert s_str.startswith("DbStats(index_stats=VectorIndexStats(")
    assert "storage_stats=StorageStats(" in s_str

def test_max_elements_config(db_path):
    # This just tests that the parameter is accepted
    db = memfuse.open(db_path, dimension=4, max_elements=5000)
    assert db is not None

def test_typed_search_results(db_path):
    db = memfuse.open(db_path, dimension=4)
    v = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    db.insert("doc1", v)

    results = db.search(v, k=1)
    assert len(results) == 1
    assert isinstance(results[0], memfuse.SearchResult)
    assert results[0].id == "doc1"

    col = db.collection("test")
    col.insert("doc2", v)
    c_results = col.search(v, k=1)
    assert len(c_results) == 1
    assert isinstance(c_results[0], memfuse.SearchResult)
    assert c_results[0].id == "doc2"
