use std::time::Duration;
use tokio::time::timeout;

/// Detects which application the user copied text from.
///
/// Strategy order (first success wins):
/// 1. KWin D-Bus via native zbus crate — no subprocess needed, fastest & most reliable on KDE Plasma 6
/// 2. KWin D-Bus via qdbus6 subprocess fallback (with 500ms timeout & graceful error handling)
/// 3. /proc fallback — inspect active process info or procfs
///
/// All strategies are graceful: None on failure, no panic.
pub async fn detect_source_app() -> Option<String> {
    // Strategy 1: Native D-Bus call via zbus
    if let Some(name) = try_kwin_dbus_native().await {
        return Some(name);
    }

    // Strategy 2: qdbus6 subprocess fallback
    if let Some(name) = try_qdbus6_kwin().await {
        return Some(name);
    }

    // Strategy 3: Procfs fallback
    if let Some(name) = try_procfs_active().await {
        return Some(name);
    }

    None
}

/// Query KWin directly via native D-Bus session bus (zbus).
async fn try_kwin_dbus_native() -> Option<String> {
    let conn = timeout(Duration::from_millis(500), zbus::Connection::session()).await.ok()?.ok()?;
    
    // Call activeWindow on org.kde.KWin /KWin
    let reply = timeout(
        Duration::from_millis(500),
        conn.call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "activeWindow",
            &(),
        )
    ).await.ok()?.ok()?;

    let window_id: String = reply.body().deserialize().ok()?;
    if window_id.is_empty() {
        return None;
    }

    // Call getWindowInfo for the window handle/UUID
    let info_reply = timeout(
        Duration::from_millis(500),
        conn.call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "getWindowInfo",
            &(window_id.clone(),),
        )
    ).await.ok()?.ok()?;

    let info_str: String = info_reply.body().deserialize().ok()?;
    parse_window_info(&info_str)
}

/// Query KWin via qdbus6 subprocess with a 500ms timeout.
async fn try_qdbus6_kwin() -> Option<String> {
    let id_cmd = tokio::process::Command::new("qdbus6")
        .args(["org.kde.KWin", "/KWin", "org.kde.KWin.activeWindow"])
        .output();

    let id_output = timeout(Duration::from_millis(500), id_cmd).await.ok()?.ok()?;

    if !id_output.status.success() {
        return None;
    }

    let window_id = String::from_utf8_lossy(&id_output.stdout).trim().to_string();
    if window_id.is_empty() {
        return None;
    }

    let info_cmd = tokio::process::Command::new("qdbus6")
        .args(["org.kde.KWin", "/KWin", "org.kde.KWin.getWindowInfo", &window_id])
        .output();

    let info_output = timeout(Duration::from_millis(500), info_cmd).await.ok()?.ok()?;

    if !info_output.status.success() {
        return None;
    }

    let info_str = String::from_utf8_lossy(&info_output.stdout);
    parse_window_info(&info_str)
}

/// Helper function to extract application name / resource class from KWin window info.
fn parse_window_info(info_str: &str) -> Option<String> {
    // 1. Try JSON parsing (Plasma 6 native getWindowInfo JSON)
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(info_str) {
        if let Some(rc) = json.get("resourceClass").and_then(|v| v.as_str()) {
            if !rc.trim().is_empty() {
                return Some(rc.trim().to_string());
            }
        }
        if let Some(rn) = json.get("resourceName").and_then(|v| v.as_str()) {
            if !rn.trim().is_empty() {
                return Some(rn.trim().to_string());
            }
        }
        if let Some(caption) = json.get("caption").and_then(|v| v.as_str()) {
            if !caption.trim().is_empty() {
                return Some(caption.trim().to_string());
            }
        }
        if let Some(pid) = json.get("pid").and_then(|v| v.as_i64()) {
            if let Some(proc_name) = read_proc_comm(pid as u32) {
                return Some(proc_name);
            }
        }
    }

    // 2. Try key-value formatted output
    for line in info_str.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("resourceClass:") {
            let val = value.trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
        if let Some(value) = line.strip_prefix("resourceName:") {
            let val = value.trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
        if let Some(value) = line.strip_prefix("pid:") {
            if let Ok(pid) = value.trim().parse::<u32>() {
                if let Some(proc_name) = read_proc_comm(pid) {
                    return Some(proc_name);
                }
            }
        }
    }

    None
}

/// Reads process name from /proc/{pid}/comm
fn read_proc_comm(pid: u32) -> Option<String> {
    let path = format!("/proc/{}/comm", pid);
    if let Ok(name) = std::fs::read_to_string(path) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Procfs / compositor fallback strategy: query hyprctl, swaymsg, or inspect /proc
async fn try_procfs_active() -> Option<String> {
    // 1. Try hyprctl on Hyprland
    let hypr_cmd = tokio::process::Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output();
    if let Ok(Ok(output)) = timeout(Duration::from_millis(300), hypr_cmd).await {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(class) = json.get("class").and_then(|v| v.as_str()) {
                    if !class.trim().is_empty() {
                        return Some(class.trim().to_string());
                    }
                }
            }
        }
    }

    // 2. Try swaymsg on Sway
    let sway_cmd = tokio::process::Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output();
    if let Ok(Ok(output)) = timeout(Duration::from_millis(300), sway_cmd).await {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(app) = find_sway_focused(&json) {
                    return Some(app);
                }
            }
        }
    }

    None
}

/// Helper to find focused app in sway window tree
fn find_sway_focused(val: &serde_json::Value) -> Option<String> {
    if val.get("focused").and_then(|v| v.as_bool()) == Some(true) {
        if let Some(app) = val.get("app_id").and_then(|v| v.as_str()) {
            if !app.trim().is_empty() { return Some(app.trim().to_string()); }
        }
        if let Some(window_props) = val.get("window_properties") {
            if let Some(class) = window_props.get("class").and_then(|v| v.as_str()) {
                if !class.trim().is_empty() { return Some(class.trim().to_string()); }
            }
        }
    }
    if let Some(nodes) = val.get("nodes").and_then(|v| v.as_array()) {
        for node in nodes {
            if let Some(found) = find_sway_focused(node) {
                return Some(found);
            }
        }
    }
    if let Some(floating) = val.get("floating_nodes").and_then(|v| v.as_array()) {
        for node in floating {
            if let Some(found) = find_sway_focused(node) {
                return Some(found);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_window_info_json() {
        let json_input = r#"{"resourceClass":"org.kde.kate","resourceName":"kate","caption":"main.rs — Kate"}"#;
        assert_eq!(parse_window_info(json_input), Some("org.kde.kate".to_string()));
    }

    #[test]
    fn test_parse_window_info_key_value() {
        let kv_input = "resourceClass: firefox\nresourceName: Navigator\npid: 12345";
        assert_eq!(parse_window_info(kv_input), Some("firefox".to_string()));
    }

    #[test]
    fn test_parse_window_info_empty() {
        assert_eq!(parse_window_info(""), None);
    }

    #[tokio::test]
    async fn test_detect_source_app_graceful() {
        // Must return Option<String> without panic regardless of OS environment
        let result = detect_source_app().await;
        let _ = result;
    }
}
