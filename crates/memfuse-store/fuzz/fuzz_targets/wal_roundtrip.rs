#![no_main]

use libfuzzer_sys::fuzz_target;
use memfuse_store::wal::{Wal, WalConfig, WalVersion};
use tempfile::tempdir;

fuzz_target!(|data: &[u8]| {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return,
    };

    rt.block_on(async {
        let dir = match tempdir() {
            Ok(d) => d,
            Err(_) => return,
        };
        let wal_path = dir.path().join("fuzz.wal");

        if tokio::fs::write(&wal_path, data).await.is_err() {
            return;
        }

        let config = WalConfig {
            allow_legacy_integrity_key_fallback: true,
            min_wal_version: WalVersion::V1,
            ..Default::default()
        };

        match Wal::open_with_config(&wal_path, config).await {
            Ok(wal) => {
                let _ = wal.replay().await;
            }
            Err(_) => {}
        }
    });
});
