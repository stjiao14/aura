use crate::commands::FriendlyError;
use tauri::command;

/// Open a specific system settings pane using the macOS `open` command.
/// This bypasses Tauri's overly strict `shell:allow-open` regex rules.
#[command]
pub fn open_system_settings(pane: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Example panes:
        // "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars"
        // "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        let status = std::process::Command::new("open")
            .arg(&pane)
            .status()
            .map_err(|e| format!("Failed to execute 'open': {}", e))?;

        if !status.success() {
            return Err(format!("'open' command failed with status: {}", status));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("System settings shortcuts are only supported on macOS".to_string())
    }
}

#[command]
pub fn get_rt_interval_secs() -> u64 {
    crate::commands::recorder::RT_INTERVAL_SECS
}

#[command]
pub async fn list_upcoming_events() -> Result<Vec<crate::calendar::CalendarEvent>, String> {
    tokio::task::spawn_blocking(crate::calendar::list_upcoming_events)
        .await
        .friendly()?
}
