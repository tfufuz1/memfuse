// FILE-CONTEXT
// ZWECK: Criterion benchmark suite for memfuse-crypto throughput and latency measurement.
// INVARIANTEN: Measures AES-256-GCM-SIV throughput at 1KB/64KB/1MB/16MB, HKDF derivation, HMAC throughput, and nonce overhead.
// STAND: TS:2026-08-30T19:50:00Z (SESSION: 20260830)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_crypto::CryptoKey;

fn bench_aes_256_gcm_siv_encrypt(c: &mut Criterion) {
    let km = CryptoKey::try_new("bench-passphrase", b"bench-salt-123456").unwrap();
    let mut group = c.benchmark_group("aes_256_gcm_siv_encrypt");

    for size in &[1024, 64 * 1024, 1024 * 1024, 16 * 1024 * 1024] {
        let payload = vec![0x42u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| km.encrypt_auto_nonce(black_box(&payload)).unwrap());
        });
    }
    group.finish();
}

fn bench_aes_256_gcm_siv_decrypt(c: &mut Criterion) {
    let km = CryptoKey::try_new("bench-passphrase", b"bench-salt-123456").unwrap();
    let mut group = c.benchmark_group("aes_256_gcm_siv_decrypt");

    for size in &[1024, 64 * 1024, 1024 * 1024, 16 * 1024 * 1024] {
        let payload = vec![0x42u8; *size];
        let (ct, nonce) = km.encrypt_auto_nonce(&payload).unwrap();
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| km.decrypt_auto_nonce(black_box(&ct), black_box(&nonce)).unwrap());
        });
    }
    group.finish();
}

fn bench_hkdf_derivation(c: &mut Criterion) {
    c.bench_function("hkdf_key_derivation_latency", |b| {
        b.iter(|| {
            CryptoKey::try_new(
                black_box("user-provided-passphrase"),
                black_box(b"salt-for-hkdf-bench-123"),
            )
            .unwrap()
        });
    });
}

fn bench_hmac_integrity(c: &mut Criterion) {
    let km = CryptoKey::try_new("bench-passphrase", b"bench-salt-123456").unwrap();
    c.bench_function("hmac_sha256_integrity_key_derivation", |b| {
        b.iter(|| km.integrity_key().unwrap());
    });
}

criterion_group!(
    benches,
    bench_aes_256_gcm_siv_encrypt,
    bench_aes_256_gcm_siv_decrypt,
    bench_hkdf_derivation,
    bench_hmac_integrity
);
criterion_main!(benches);
