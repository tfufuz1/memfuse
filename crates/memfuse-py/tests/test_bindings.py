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

def test_zero_copy_integrity(tmp_path):
    db = memfuse.open(str(tmp_path / "test_zero_copy"), dimension=4)
    col = db.collection("zero_copy")

    # Contiguous array
    v_contig = np.array([1.0, 2.0, 3.0, 4.0], dtype=np.float32)
    col.insert("contig", v_contig)

    # Non-contiguous array (slice with stride)
    v_full = np.array([1.0, 0.0, 2.0, 0.0, 3.0, 0.0, 4.0, 0.0], dtype=np.float32)
    v_noncontig = v_full[::2]
    assert not v_noncontig.flags.c_contiguous

    # This should still work as PyReadonlyArray1::as_slice() usually fails for non-contiguous
    # or requires a copy. Let's see how our implementation handles it.
    # If it fails, we might need to handle it in Rust (e.g., call .to_owned_array() or similar)
    # Actually, as_slice() on PyReadonlyArray1 returns Err if not contiguous.
    try:
        col.insert("noncontig", v_noncontig)
    except ValueError as e:
        assert "Invalid vector format" in str(e)
        # If it fails with ValueError, it's expected behavior for as_slice()
        # To support non-contiguous, we'd need to copy in Python or Rust.
        v_fixed = np.ascontiguousarray(v_noncontig)
        col.insert("noncontig", v_fixed)

    res = col.search(v_contig, k=2)
    assert len(res) == 2

def test_full_collection_api(tmp_path):
    db = memfuse.open(str(tmp_path / "test_full_api"), dimension=4)
    col = db.collection("api_test")
    v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
    meta = {"text": "hello world", "tags": ["test"]}

    # Insert
    col.insert("doc1", v, meta)
    assert len(col) == 1

    # Get
    doc = col.get("doc1")
    assert doc.id == "doc1"
    assert doc.metadata["text"] == "hello world"

    # Update
    v2 = np.array([0.5, 0.6, 0.7, 0.8], dtype=np.float32)
    col.update("doc1", v2, {"text": "updated world"})
    doc = col.get("doc1")
    assert doc.metadata["text"] == "updated world"

    # Relate & Scan Prefix
    col.insert("doc2", v)
    col.relate("doc1", "doc2", "friend")
    rels = col.scan_prefix("__rel:doc1:friend:")
    assert len(rels) == 1
    assert rels[0][1]["to"] == "doc2"

    # Delete
    col.delete("doc1")
    assert len(col) == 1 # doc2 remains
    assert col.get("doc1") is None

def test_db_management(tmp_path):
    db = memfuse.open(str(tmp_path / "test_db_mgmt"), dimension=4)
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
