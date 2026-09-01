import memfuse
import numpy as np
import pytest
import random
import string

@pytest.fixture
def db(tmp_path):
    path = str(tmp_path / "fuzz_db")
    return memfuse.open(path, dimension=8)

def test_fuzz_invalid_open_parameters(tmp_path):
    path = str(tmp_path / "fuzz_open_db")

    # Path validation
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        memfuse.open("", dimension=8)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        memfuse.open("   ", dimension=8)

    # Dimension bounds
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        memfuse.open(path, dimension=0)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        memfuse.open(path, dimension=10001)

    # Max elements bound
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        memfuse.open(path, dimension=8, max_elements=0)

    # Distance metric validation
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        memfuse.open(path, dimension=8, distance_metric="unsupported_metric")

def test_fuzz_insert_and_upsert_boundaries(db):
    v8 = np.zeros(8, dtype=np.float32)

    # ID boundary validation
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert("", v8)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert("   ", v8)

    long_id = "a" * 1025
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert(long_id, v8)

    # Vector boundary validation
    v_empty = np.array([], dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert("doc_empty_v", v_empty)

    v_nan = np.array([1.0, float('nan'), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert("doc_nan", v_nan)

    v_inf = np.array([1.0, float('inf'), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert("doc_inf", v_inf)

    v_neginf = np.array([1.0, float('-inf'), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert("doc_neginf", v_neginf)

    # Dimension mismatch
    v4 = np.zeros(4, dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert("doc_dim_mismatch", v4)

def test_fuzz_search_and_search_fb_boundaries(db):
    v8 = np.ones(8, dtype=np.float32)
    db.insert("doc1", v8)

    # k bounds
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search(v8, k=0)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search(v8, k=1001)

    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search_fb(v8, k=0)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search_fb(v8, k=1001)

    # Invalid vector in search
    v_nan = np.array([float('nan')] * 8, dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search(v_nan, k=5)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.search_fb(v_nan, k=5)

def test_fuzz_hybrid_search_and_fb_boundaries(db):
    v8 = np.ones(8, dtype=np.float32)
    db.insert("doc1", v8)

    # Text query bounds
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.hybrid_search("", v8, k=5)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.hybrid_search("   ", v8, k=5)

    long_text = "q" * 1025
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.hybrid_search(long_text, v8, k=5)

    # Partial weight specification
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.hybrid_search("query", v8, k=5, vector_weight=0.5, text_weight=0.5) # missing graph_weight

    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.hybrid_search_fb("query", v8, k=5, vector_weight=0.5)

def test_fuzz_relate_boundaries(db):
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.relate("", "doc2", "knows")
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.relate("doc1", "", "knows")
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.relate("doc1", "doc2", "")
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.relate("doc1", "doc2", "   ")

    long_label = "r" * 257
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.relate("doc1", "doc2", long_label)

def test_fuzz_batch_operations_boundaries(db):
    # Empty batch
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert_many([])
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.upsert_many([])

    # Batch item with invalid vector
    v_bad = np.array([1.0, float('nan'), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], dtype=np.float32)
    with pytest.raises((memfuse.MemFuseValueError, ValueError)):
        db.insert_many([("doc1", v_bad, None)])

def test_random_fuzz_property_never_panics_or_crashes(db):
    """Property test: random malformed/extreme inputs must raise controlled exceptions and never crash process."""
    random.seed(42)
    for _ in range(30):
        rand_id = ''.join(random.choices(string.printable, k=random.randint(0, 100)))
        rand_dim = random.randint(0, 16)

        # Generate vector with potential NaNs/Infs
        vals = [random.uniform(-100, 100) for _ in range(rand_dim)]
        if random.random() < 0.2 and vals:
            vals[random.randint(0, len(vals) - 1)] = float('nan')
        elif random.random() < 0.2 and vals:
            vals[random.randint(0, len(vals) - 1)] = float('inf')

        vec = np.array(vals, dtype=np.float32)

        # Call FFI boundary - must cleanly succeed or raise Exception, NEVER crash
        try:
            db.insert(rand_id, vec)
        except Exception:
            pass

        try:
            db.search(vec, k=random.randint(0, 10))
        except Exception:
            pass
