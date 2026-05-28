import memfuse
import numpy as np
import pytest
import os
import shutil

@pytest.fixture
def db_path(tmp_path):
    path = str(tmp_path / "test_db_filter")
    yield path
    if os.path.exists(path):
        shutil.rmtree(path)

def test_search_with_filter(db_path):
    db = memfuse.open(db_path, dimension=4)
    v1 = np.array([1.0, 0.0, 0.0, 0.0], dtype=np.float32)
    v2 = np.array([0.0, 1.0, 0.0, 0.0], dtype=np.float32)

    db.insert("doc1", v1, metadata={"category": "A", "value": 10})
    db.insert("doc2", v2, metadata={"category": "B", "value": 20})

    # Filter for category A
    filt = {"Condition": {"field": "category", "op": "Eq", "value": "A"}}
    results = db.search_with_filter(v1, k=10, filter=filt)
    assert len(results) == 1
    assert results[0].id == "doc1"

    # Filter for value > 15
    filt2 = {"Condition": {"field": "value", "op": "Gt", "value": 15}}
    results2 = db.search_with_filter(v1, k=10, filter=filt2)
    assert len(results2) == 1
    assert results2[0].id == "doc2"

def test_exception_mapping(db_path):
    db = memfuse.open(db_path, dimension=4)
    # Invalid dimension should raise ValueError (mapped from InvalidInput)
    v_bad = np.array([1.0, 0.0], dtype=np.float32)
    with pytest.raises(ValueError) as excinfo:
        db.insert("bad", v_bad)
    assert "Dimension mismatch" in str(excinfo.value)
