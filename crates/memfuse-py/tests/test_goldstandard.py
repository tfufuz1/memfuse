import memfuse
import numpy as np
import pytest
import os
import shutil

@pytest.fixture
def db_path(tmp_path):
    path = str(tmp_path / "gold_db")
    yield path
    if os.path.exists(path):
        shutil.rmtree(path)

def test_stategraph_api():
    graph = memfuse.StateGraph()

    # Test Node with tool and params
    node_retrieve = memfuse.Node(
        "retrieve",
        "Retrieves context from database",
        tool="memfuse.hybrid_search",
        params={"limit": 10, "weights": {"vector": 0.6, "text": 0.4}}
    )

    node_synthesize = memfuse.Node(
        "synthesize",
        "Synthesizes answer using LLM",
        tool="llm.complete",
        params={"model": "claude-sonnet-4-20250514"}
    )

    graph.add_node(node_retrieve)
    graph.add_node(node_synthesize)

    graph.add_edge("retrieve", "synthesize", condition="always")
    graph.add_edge("synthesize", "retrieve", condition="needs_more_context")

    # Should not crash
    graph.run(initial_state="retrieve")

def test_airgap_config_and_verifier():
    # Strict airgap
    config = memfuse.AirGapConfig.strict()
    config.validate()

    report = memfuse.AirGapVerifier.verify(config)
    assert report.is_compliant()
    assert report.network_isolated
    assert report.encryption_active

    # Airgap with local model
    config_model = memfuse.EmbeddingProvider.local(model_path="/models/e5-large-v2.onnx")
    config_model.validate()

    # Test repr
    assert "AirGapReport" in repr(report)

def test_open_with_airgap(db_path):
    config = memfuse.AirGapConfig(require_encryption=True)

    # Should fail if require_encryption=True but no passphrase provided
    with pytest.raises(ValueError, match="Air-gap mode requires encryption passphrase"):
        memfuse.open(db_path, dimension=4, airgap=config)

    # Should succeed with passphrase
    db = memfuse.open(db_path, dimension=4, encryption_passphrase="secret-passphrase", airgap=config)
    assert db is not None
