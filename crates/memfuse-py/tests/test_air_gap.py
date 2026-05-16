import memfuse
import os

def test_air_gap_profile_parameters(tmp_path):
    db_path = str(tmp_path / "air_gap_db")

    # Default parameters
    db = memfuse.open(db_path, dimension=4)
    assert db.encryption == False
    assert db.network == True
    assert db.embedding is None

    # Custom air-gap parameters
    provider = memfuse.EmbeddingProvider.local("/models/e5-large.onnx", runtime="ort")
    db2 = memfuse.open(db_path + "2", dimension=4, encryption=True, network=False, embedding=provider)

    assert db2.encryption == True
    assert db2.network == False
    assert db2.embedding.model_path == "/models/e5-large.onnx"
    assert db2.embedding.runtime == "ort"
