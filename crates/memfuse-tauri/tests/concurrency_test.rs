use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_tauri_lib::commands::{
    create_collection, list_collections, run_bulk_regex_transform, validate_regex_pattern,
};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_app_state_concurrent_operations() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let app_state = Arc::new(memfuse_tauri_lib::state::AppState::new());
    *app_state.db.write() = Some(Arc::new(db));
    *app_state.db_path.write() = Some(db_path);

    let mut handles = Vec::new();

    // 1. Concurrent collection creation
    for i in 0..10 {
        let state_ref = app_state.clone();
        let name = format!("col_{i}");
        let handle = tokio::spawn(async move {
            // SAFETY: In concurrent unit tests, `state_ref` is an Arc<AppState> that remains alive throughout the test execution.
            // Transmuting the reference into `tauri::State` simulates Tauri's state injection framework safely.
            let state: tauri::State<'_, memfuse_tauri_lib::state::AppState> =
                unsafe { std::mem::transmute(&*state_ref) };
            create_collection(state, name).await.map(|_| ())
        });
        handles.push(handle);
    }

    // 2. Concurrent regex validations & transforms
    for i in 0..10 {
        let state_ref = app_state.clone();
        let handle = tokio::spawn(async move {
            let val = validate_regex_pattern(format!(r"\bword_{i}\b"));
            assert!(val.is_valid);

            // SAFETY: In concurrent unit tests, `state_ref` is an Arc<AppState> that remains alive throughout the test execution.
            // Transmuting the reference into `tauri::State` simulates Tauri's state injection framework safely.
            let state: tauri::State<'_, memfuse_tauri_lib::state::AppState> =
                unsafe { std::mem::transmute(&*state_ref) };
            let res = run_bulk_regex_transform(
                state,
                format!("word_{i}"),
                "g".into(),
                "REPLACED".into(),
                vec![format!("hello word_{i} test")],
            )
            .await;
            assert!(res.is_ok());
            let transform_res = res.unwrap().pop().unwrap().unwrap();
            assert_eq!(transform_res.output, "hello REPLACED test");
            Ok(())
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await.unwrap();
    }

    // SAFETY: In unit tests, `app_state` is an Arc<AppState> owned by this thread and kept alive.
    // Transmuting the reference into `tauri::State` simulates Tauri's state injection framework safely.
    let state: tauri::State<'_, memfuse_tauri_lib::state::AppState> =
        unsafe { std::mem::transmute(&*app_state) };
    let cols = list_collections(state).await.expect("list_collections");
    // 1 default collection + 10 created collections = 11
    assert_eq!(cols.len(), 11);
    for i in 0..10 {
        assert!(cols.iter().any(|c| c.name == format!("col_{i}")));
    }
}
