use memfuse_crypto::crypto::KeyManager;
#[test]
fn test_crypto() {
    let km = KeyManager::try_new("pass").unwrap(); // unwrap
    let enc = km.encrypt(b"data", 0).unwrap(); // unwrap
    let dec = km.decrypt(&enc, 0).unwrap(); // unwrap
    assert_eq!(dec, b"data");
}
