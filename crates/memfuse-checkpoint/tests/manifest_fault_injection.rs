use memfuse_checkpoint::{CheckpointManifest, CheckpointMeta};
use memfuse_core::TxId;

fn sample_meta() -> CheckpointMeta {
    CheckpointMeta {
        name: "test_cp".to_string(),
        collection_id: "col_1".to_string(),
        seq_no: 42,
        tx_id: TxId::new(100),
        metadata: serde_json::json!({"step": 1}),
        created_at: 1000,
    }
}

/// Fault-Injection: Partial-Write-Simulation — verifiziert dass ein
/// Manifest mit getrennten Meta/Components-Teilen nicht als valid akzeptiert wird
#[test]
fn test_manifest_partial_write_rejected() {
    let meta = sample_meta();
    let components = vec!["storage".to_string(), "index".to_string()];
    let mut manifest = CheckpointManifest::new(meta, components).unwrap();

    // Manipuliere components NACH der Checksum-Berechnung
    manifest.components.push("corrupted_component".to_string());

    // verify() MUSS Err(MemFuseError::Serialization(_)) zurückgeben
    assert!(
        manifest.verify().is_err(),
        "Manifest with tampered components must fail verification"
    );
}

/// Tamper-Scenario: Checksum-Manipulation
#[test]
fn test_manifest_tampered_checksum_rejected() {
    let meta = sample_meta();
    let components = vec!["storage".to_string()];
    let mut manifest = CheckpointManifest::new(meta, components).unwrap();

    manifest.checksum = "0".repeat(64); // Manipulierte Checksum
    assert!(
        manifest.verify().is_err(),
        "Manifest with invalid checksum must fail verification"
    );
}

/// Round-Trip-Invariante
#[test]
fn test_manifest_roundtrip_json_stable() {
    let meta = sample_meta();
    let original = CheckpointManifest::new(meta, vec!["storage".into(), "index".into()]).unwrap();
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: CheckpointManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(original, deserialized);
    assert!(deserialized.verify().is_ok());
}

/// Empty-Components-Edge-Case
#[test]
fn test_manifest_empty_components_valid() {
    let meta = sample_meta();
    let manifest = CheckpointManifest::new(meta, vec![]).unwrap();
    assert!(manifest.verify().is_ok());
}

/// Fault-Injection: Simulierter Absturz zwischen zwei Schreibschritten als Testkriterium
#[test]
fn test_manifest_crash_between_writes() {
    let meta = sample_meta();
    // Simulate partial JSON write (e.g. system crashed before closing braces)
    let json = format!(
        "{{\"meta\":{}, \"components\":[\"storage\"",
        serde_json::to_string(&meta).unwrap()
    );
    let res: Result<CheckpointManifest, _> = serde_json::from_str(&json);
    assert!(res.is_err(), "Partial write MUST be rejected");
}
