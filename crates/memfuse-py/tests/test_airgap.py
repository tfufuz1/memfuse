import memfuse
import numpy as np
import pytest
import os
import shutil

@pytest.fixture
def db_path(tmp_path):
    path = str(tmp_path / "airgap_db")
    yield path
    if os.path.exists(path):
        shutil.rmtree(path)

def test_embedding_provider_scaffold(db_path):
    # Test that we can create an EmbeddingProvider and pass it to open
    provider = memfuse.EmbeddingProvider.local(model_path="/tmp/mock.onnx", runtime="ort")
    db = memfuse.open(db_path, dimension=4, embedding_provider=provider)

    # embed() should currently raise NotImplementedError as it's a scaffold
    with pytest.raises(RuntimeError) as excinfo:
        provider.embed("hello")
    assert "not yet fully integrated" in str(excinfo.value)

def test_network_flag(db_path):
    # network=False should be passed to the Rust config
    db = memfuse.open(db_path, dimension=4, network=False)
    # Since it's currently a no-op in the embedded DB, we just verify it doesn't crash
    assert db is not None

def test_text_to_vector_resolution_failure_without_provider(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("test")

    # insert without vector and without provider should fail
    with pytest.raises(ValueError) as excinfo:
        col.insert("doc1", text="some text")
    assert "Either 'vector' or 'text' (with configured EmbeddingProvider) must be provided." in str(excinfo.value)

def test_search_resolution_failure_without_provider(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("test")

    with pytest.raises(ValueError) as excinfo:
        col.search(text="some query")
    assert "Either 'vector' or 'text' (with configured EmbeddingProvider) must be provided." in str(excinfo.value)

def test_hybrid_search_resolution_failure_without_provider(db_path):
    db = memfuse.open(db_path, dimension=4)
    col = db.collection("test")

    with pytest.raises(ValueError) as excinfo:
        col.hybrid_search(text="some query")
    assert "Either 'vector' or configured EmbeddingProvider must be provided." in str(excinfo.value)
