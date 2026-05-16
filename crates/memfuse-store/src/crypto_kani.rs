//! Kani formal verification proofs for MemFuse Crypto.

#[cfg(kani)]
mod proofs {
    use crate::crypto::KeyManager;

    #[kani::proof]
    #[kani::unwind(32)]
    fn proof_encrypt_decrypt_identity() {
        let passphrase = "test-passphrase";
        let km = KeyManager::new(passphrase);

        let data_len: usize = kani::any();
        kani::assume(data_len <= 16); // Constrain for performance in verification

        let data: Vec<u8> = (0..data_len).map(|_| kani::any()).collect();
        let nonce: u64 = kani::any();

        if let Ok(encrypted) = km.encrypt(&data, nonce) {
            if let Ok(decrypted) = km.decrypt(&encrypted, nonce) {
                assert_eq!(data, decrypted);
            } else {
                panic!("Decryption failed for successfully encrypted data");
            }
        }
    }

    #[kani::proof]
    fn proof_wrong_nonce_fails_decryption() {
        let passphrase = "test-passphrase";
        let km = KeyManager::new(passphrase);

        let data: [u8; 8] = kani::any();
        let nonce: u64 = kani::any();
        let wrong_nonce: u64 = kani::any();
        kani::assume(nonce != wrong_nonce);

        if let Ok(encrypted) = km.encrypt(&data, nonce) {
            let res = km.decrypt(&encrypted, wrong_nonce);
            assert!(res.is_err(), "Decryption should fail with wrong nonce");
        }
    }
}
