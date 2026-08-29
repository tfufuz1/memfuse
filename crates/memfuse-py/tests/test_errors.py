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

def test_empty_id_validation(db):
    v = np.zeros(4, dtype=np.float32)
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert("", v)
    assert "Document ID cannot be empty" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.get("")
    assert "Document ID cannot be empty" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.delete("")
    assert "Document ID cannot be empty" in str(excinfo.value)

def test_nan_vector_validation(db):
    v_nan = np.array([1.0, float('nan'), 0.0, 0.0], dtype=np.float32)
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert("doc_nan", v_nan)
    assert "NaN or infinite" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.search(v_nan, k=5)
    assert "NaN or infinite" in str(excinfo.value)

def test_inf_vector_validation(db):
    v_inf = np.array([1.0, float('inf'), 0.0, 0.0], dtype=np.float32)
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert("doc_inf", v_inf)
    assert "NaN or infinite" in str(excinfo.value)

def test_relate_validation(db):
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.relate("", "doc2", "knows")
    assert "Document ID cannot be empty" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.relate("doc1", "doc2", "")
    assert "label cannot be empty" in str(excinfo.value)
