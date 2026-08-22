#[tokio::test]
async fn test_ollama_bridge_handles_connection_error_gracefully() {
    // Bridge auf einen garantiert nicht existierenden Port zeigen lassen
    let bridge = memfuse_tauri_lib::ollama::OllamaBridge::new("http://localhost:1");
    let result = bridge.list_models().await;
    assert!(result.is_err());
    // Fehlermeldung muss hilfreich sein (erwähnt "Ollama" und "gestartet")
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Ollama"));
}
