import tempfile
import numpy as np
import pytest
import memfuse

def test_id_length_boundary_validation():
    with tempfile.TemporaryDirectory() as tmpdir:
        db = memfuse.open(tmpdir, dimension=4)

        # ID exceeds MAX_ID_LENGTH (1024 chars)
        long_id = "a" * 1025
        v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
        with pytest.raises((ValueError, memfuse.MemFuseValueError)):
            db.insert(long_id, v)

        # Query text exceeds MAX_ID_LENGTH
        long_text = "q" * 1025
        with pytest.raises((ValueError, memfuse.MemFuseValueError)):
            db.hybrid_search(long_text, v, k=5)

        # Relate label exceeds MAX_LABEL_LENGTH (256 chars)
        db.insert("doc1", v)
        db.insert("doc2", v)
        long_label = "r" * 257
        with pytest.raises((ValueError, memfuse.MemFuseValueError)):
            db.relate("doc1", "doc2", long_label)

def test_batch_size_limits():
    with tempfile.TemporaryDirectory() as tmpdir:
        db = memfuse.open(tmpdir, dimension=4)
        v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)

        # Batch size exceeds 10,000 items
        too_many_docs = [("id_" + str(i), v, None) for i in range(10001)]
        with pytest.raises((ValueError, memfuse.MemFuseValueError)):
            db.insert_many(too_many_docs)

        with pytest.raises((ValueError, memfuse.MemFuseValueError)):
            db.upsert_many(too_many_docs)

def test_search_k_boundary():
    with tempfile.TemporaryDirectory() as tmpdir:
        db = memfuse.open(tmpdir, dimension=4)
        v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)

        with pytest.raises(ValueError):
            db.search(v, k=0)

        with pytest.raises(ValueError):
            db.search(v, k=1001)

def test_hybrid_search_partial_weights():
    with tempfile.TemporaryDirectory() as tmpdir:
        db = memfuse.open(tmpdir, dimension=4)
        v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)

        # Passing partial weights (2 instead of all 3 or none)
        with pytest.raises(ValueError):
            db.hybrid_search("query", v, k=5, vector_weight=0.5, text_weight=0.5)

def test_flatbuffer_search_fb_and_hybrid_fb():
    with tempfile.TemporaryDirectory() as tmpdir:
        db = memfuse.open(tmpdir, dimension=4)
        v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
        db.insert("doc1", v, {"tag": "fb_test"})

        res_bytes = db.search_fb(v, k=5)
        assert isinstance(res_bytes, bytes)
        assert len(res_bytes) > 0

        hybrid_bytes = db.hybrid_search_fb("doc1", v, k=5)
        assert isinstance(hybrid_bytes, bytes)
        assert len(hybrid_bytes) > 0

def test_context_manager_protocol():
    with tempfile.TemporaryDirectory() as tmpdir:
        with memfuse.open(tmpdir, dimension=4) as db:
            v = np.array([0.1, 0.2, 0.3, 0.4], dtype=np.float32)
            db.insert("cm_doc", v)

        # Reopen to confirm data was flushed during context exit
        db2 = memfuse.open(tmpdir, dimension=4)
        doc = db2.get("cm_doc")
        assert doc is not None
        assert doc.id == "cm_doc"
