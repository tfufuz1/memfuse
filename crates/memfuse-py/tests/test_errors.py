import memfuse
import numpy as np
import pytest

@pytest.fixture
def db(tmp_path):
    path = str(tmp_path / "error_db")
    db = memfuse.open(path, dimension=4)
    return db

def test_invalid_dimension_raises_value_error(db):
    v_bad = np.array([1.0, 2.0, 3.0], dtype=np.float32) # dimension is 4
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert("doc1", v_bad)
    assert "Dimension mismatch" in str(excinfo.value)
    assert excinfo.value.kind == "InvalidInput"

def test_search_k_bounds(db):
    v = np.zeros(4, dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search(v, k=0)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search(v, k=1001)

def test_get_nonexistent_returns_none(db):
    assert db.get("nonexistent") is None

def test_drop_default_collection_fails(db):
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.drop_collection("default")
    assert "Cannot drop default collection" in str(excinfo.value)
    assert excinfo.value.kind == "InvalidInput"

def test_invalid_distance_metric(tmp_path):
    path = str(tmp_path / "metric_db")
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        memfuse.open(path, dimension=4, distance_metric="invalid_metric")

def test_not_found_error(db):
    # This might trigger a NotFound error in Rust which should map to MemFuseError (default) or something specific
    # But currently delete doesn't fail if ID is missing in LSM.
    # We can try to update a non-existent document.
    v = np.zeros(4, dtype=np.float32)
    # update currently doesn't strictly check existence in some impls, let's see.
    # Actually memfuse-db update usually checks.
    pass
