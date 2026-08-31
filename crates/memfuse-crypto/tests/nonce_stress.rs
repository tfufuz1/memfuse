// FILE-CONTEXT
// ZWECK: Multi-threaded nonce uniqueness stress testing & empirical vs theoretical collision probability verification.
// INVARIANTEN: 1,000,000 nonces generated via KeyManager::encrypt_auto_nonce across parallel threads must be distinct (0 collisions).
// NICHT-OFFENSICHTLICH: Uses Birthday Paradox math calculation: p ~ 1 - exp(-n^2 / (2 * 2^64)) for 64-bit OsRng suffix.
// STAND: TS:2026-08-30T19:35:00Z (SESSION: 20260830)

use memfuse_crypto::CryptoKey;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_1m_nonce_uniqueness_stress() {
    const TOTAL_NONCES: usize = 1_000_000;
    const NUM_THREADS: usize = 10;
    const NONCES_PER_THREAD: usize = TOTAL_NONCES / NUM_THREADS;

    let km = Arc::new(CryptoKey::try_new("nonce-stress-passphrase", b"salt-stress-9999").expect("CryptoKey init"));
    let nonces_set = Arc::new(Mutex::new(HashSet::with_capacity(TOTAL_NONCES)));

    let mut handles = Vec::new();
    for _ in 0..NUM_THREADS {
        let km_clone = Arc::clone(&km);
        let nonces_clone = Arc::clone(&nonces_set);
        handles.push(tokio::spawn(async move {
            let data = b"stress payload";
            let mut local_nonces = Vec::with_capacity(NONCES_PER_THREAD);
            for _ in 0..NONCES_PER_THREAD {
                let (_, nonce) = km_clone.encrypt_auto_nonce(data).expect("encrypt");
                local_nonces.push(nonce);
            }
            let mut guard = nonces_clone.lock().expect("lock nonces_set");
            for nonce in local_nonces {
                assert!(guard.insert(nonce), "CRITICAL: Nonce collision detected!");
            }
        }));
    }

    for handle in handles {
        handle.await.expect("thread finished successfully");
    }

    let total_unique = nonces_set.lock().expect("lock nonces_set").len();
    assert_eq!(
        total_unique, TOTAL_NONCES,
        "Expected {} unique nonces, got {}",
        TOTAL_NONCES, total_unique
    );

    // Mathematical birthday paradox collision probability verification:
    // n = 1,000,000 nonces generated.
    // 64-bit random suffix space d = 2^64 ~ 1.8446744e19.
    // Birthday paradox approximation for collision probability: p ~ 1 - exp(-n^2 / (2 * d))
    let n = TOTAL_NONCES as f64;
    let d = (2.0f64).powi(64);
    let p_collision = 1.0 - (- (n * n) / (2.0 * d)).exp();

    println!("--- NONCE UNICITY STRESS TEST STATS ---");
    println!("Total Nonces Tested: {}", TOTAL_NONCES);
    println!("Unique Nonces Generated: {}", total_unique);
    println!("Empirical Collision Count: 0");
    println!("Theoretical Birthday Collision Probability (64-bit suffix): {:.10e}", p_collision);
    println!("---------------------------------------");

    assert!(p_collision < 1e-7, "Theoretical collision probability should be extremely low");
}

#[test]
fn test_multi_instance_prefix_isolation() {
    const INSTANCES: usize = 100;
    let mut prefixes = HashSet::new();

    for _ in 0..INSTANCES {
        let km = CryptoKey::try_new("passphrase", b"salt").expect("CryptoKey init");
        let (_, nonce) = km.encrypt_auto_nonce(b"test").expect("encrypt");
        let prefix = [nonce[0], nonce[1], nonce[2], nonce[3]];
        prefixes.insert(prefix);
    }

    // 100 random 4-byte prefixes should all be distinct with probability > 99.998%
    assert_eq!(prefixes.len(), INSTANCES, "All instance prefixes must be distinct");
}
