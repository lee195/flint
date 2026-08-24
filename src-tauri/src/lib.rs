use common::config;

/// Bridge-works test — removed in Phase 0 once the probe command round-trips.
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! Flint data dir: {}", config::data_dir().display())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            // Ensure the canonical data dir exists. Best-effort — must not block
            // launch. `~/.flint/` is owned by common::config::data_dir() only
            // (never Tauri's app_data_dir).
            let _ = std::fs::create_dir_all(config::data_dir());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
