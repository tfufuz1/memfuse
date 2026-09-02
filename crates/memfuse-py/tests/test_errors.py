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
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search(v, k=0)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search(v, k=1001)

def test_empty_collection_name_and_query_validation(db):
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.collection("")
    assert "Collection name cannot be empty" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.hybrid_search("", v, k=1)
    assert "Search query text cannot be empty" in str(excinfo.value)

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
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert("", v)
    assert "Document ID cannot be empty" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.get("")
    assert "Document ID cannot be empty" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.delete("")
    assert "Document ID cannot be empty" in str(excinfo.value)

def test_null_byte_validation(db, tmp_path):
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert("doc\0id", v)
    assert "cannot contain null bytes" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.get("doc\0id")
    assert "cannot contain null bytes" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.delete("doc\0id")
    assert "cannot contain null bytes" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.collection("col\0name")
    assert "cannot contain null bytes" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        memfuse.open(str(tmp_path) + "\0db", dimension=4)
    assert "cannot contain null bytes" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.hybrid_search("query\0text", v, k=1)
    assert "cannot contain null bytes" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.relate("doc1", "doc2", "label\0byte")
    assert "cannot contain null bytes" in str(excinfo.value)

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

def test_long_id_validation(db):
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    long_id = "a" * 1025
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert(long_id, v)
    assert "exceeds maximum length of 1024 bytes" in str(excinfo.value)

def test_numeric_id_validation(db):
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)

    # Non-negative integer IDs should be accepted and converted to string
    db.insert(12345, v, metadata={"numeric": True})
    doc = db.get(12345)
    assert doc is not None
    assert doc.id == "12345"

    # Negative integer IDs must raise MemFuseValueError
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert(-1, v)
    assert "cannot be a negative integer" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.get(-42)
    assert "cannot be a negative integer" in str(excinfo.value)

    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.delete(-100)
    assert "cannot be a negative integer" in str(excinfo.value)

    # Overflowing integer IDs (> u64::MAX) must raise MemFuseValueError
    overflow_id = 2**128 + 1
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.insert(overflow_id, v)
    assert "exceeds maximum allowed bound" in str(excinfo.value)

    huge_overflow_id = 10**100
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.get(huge_overflow_id)
    assert "exceeds maximum allowed bound" in str(excinfo.value)

def test_long_collection_name_validation(db):
    long_name = "c" * 65
    with pytest.raises(memfuse.MemFuseValueError) as excinfo:
        db.collection(long_name)
    assert "exceeds maximum length of 64 bytes" in str(excinfo.value)
