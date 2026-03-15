mod audio;
mod calendar;
mod commands;
mod models;
mod session;
mod storage;
mod summarize;
mod transcribe;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};
use tracing_subscriber::EnvFilter;

/// Set to true while a native file dialog is open to prevent the auto-hide
/// focus-loss handler from hiding the window behind the dialog.
pub(crate) static SUPPRESS_AUTOHIDE: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        // Show the window and bring to front
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                            #[cfg(target_os = "macos")]
                            {
                                use objc2::{class, msg_send};
                                unsafe {
                                    let ns_app: *mut objc2::runtime::AnyObject =
                                        msg_send![class!(NSApplication), sharedApplication];
                                    let _: () = msg_send![ns_app, activateIgnoringOtherApps: true];
                                }
                            }
                        }
                        // Tell the frontend to toggle recording
                        let _ = app.emit("hotkey:toggle", ());
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Hide from Dock — Aura lives in the menu bar only
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Initialize DB
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_dir)?;
            storage::init_db(&app_dir)?;

            // Mark any sessions left in recording/processing state as errored
            // (happens when the app crashed or was force-quit mid-session)
            if let Ok(conn) = storage::open(&app_dir) {
                let _ = conn.execute(
                    "UPDATE sessions SET status = 'error', error_message = 'Recording interrupted (app was closed)'
                     WHERE status IN ('recording', 'processing')",
                    [],
                );
            }

            app.manage(storage::AppDataDir(app_dir.clone()));
            app.manage(commands::session::ActiveSessions(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )));



            #[cfg(target_os = "macos")]
            setup_tray(app)?;

            // Force macOS rounded corners: set NSWindow opaque=false + clear background,
            // then clip the contentView layer to a rounded rect. This makes the corner
            // areas genuinely transparent (showing the desktop) regardless of whether
            // Tauri's transparent:true config fully propagated.
            #[cfg(target_os = "macos")]
            {
                use objc2::{class, msg_send};
                unsafe {
                    let ns_app: *mut objc2::runtime::AnyObject =
                        msg_send![class!(NSApplication), sharedApplication];
                    let windows: *mut objc2::runtime::AnyObject =
                        msg_send![ns_app, windows];
                    let count: usize = msg_send![windows, count];
                    for i in 0..count {
                        let ns_window: *mut objc2::runtime::AnyObject =
                            msg_send![windows, objectAtIndex: i];
                        // Force the window to be transparent
                        let _: () = msg_send![ns_window, setOpaque: false];
                        let clear: *mut objc2::runtime::AnyObject =
                            msg_send![class!(NSColor), clearColor];
                        let _: () = msg_send![ns_window, setBackgroundColor: clear];
                        // Clip the contentView (which contains the WKWebView) to rounded corners
                        let cv: *mut objc2::runtime::AnyObject =
                            msg_send![ns_window, contentView];
                        let _: () = msg_send![cv, setWantsLayer: true];
                        let layer: *mut objc2::runtime::AnyObject =
                            msg_send![cv, layer];
                        let _: () = msg_send![layer, setCornerRadius: 14.0_f64];
                        let _: () = msg_send![layer, setMasksToBounds: true];
                        // Draw shadow following the rounded visible shape
                        let _: () = msg_send![ns_window, setHasShadow: true];
                    }
                }
            }

            // Register global hotkey from saved config (or default)
            {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                let conn = storage::open(&app_dir)?;
                let config = commands::models::load_model_config(&conn)
                    .unwrap_or_default();
                let hotkey = config
                    .hotkey_toggle
                    .as_deref()
                    .unwrap_or("CmdOrCtrl+Shift+R");
                app.global_shortcut().register(hotkey)?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session::start_session,
            commands::session::stop_session,
            commands::session::list_sessions,
            commands::session::get_session,
            commands::session::delete_session,
            commands::session::export_session,
            commands::session::rename_session,
            commands::session::update_transcript_chunk,
            commands::session::update_session_notes,
            commands::models::get_model_config,
            commands::models::set_model_config,
            commands::models::download_model,
            commands::models::check_model_downloaded,
            commands::models::pick_model_file,
            commands::system::list_upcoming_events,
            commands::system::open_system_settings,
            commands::system::get_rt_interval_secs,
            commands::models::test_provider_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Aura");
}

#[cfg(target_os = "macos")]
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

    let quit_item = MenuItem::with_id(app, "quit", "Quit Aura", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit_item])?;

    let tray = app.tray_by_id("main").expect("tray 'main' not found");
    tray.set_menu(Some(menu))?;
    tray.set_show_menu_on_left_click(false)?;

    let window = app.get_webview_window("main").unwrap();

    // Auto-hide when window loses focus (click outside),
    // unless a native dialog is open (SUPPRESS_AUTOHIDE).
    let window_blur = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            if !crate::SUPPRESS_AUTOHIDE.load(Ordering::SeqCst) {
                let _ = window_blur.hide();
            }
        }
    });

    // Left-click: toggle window, positioned below tray icon
    let window_click = window.clone();
    tray.on_tray_icon_event(move |_tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            position,
            ..
        } = event
        {
            if window_click.is_visible().unwrap_or(false) {
                let _ = window_click.hide();
            } else {
                // Position the popup just below the tray icon
                let win_size = window_click.outer_size().unwrap_or(tauri::PhysicalSize {
                    width: 380,
                    height: 600,
                });
                let x = (position.x - win_size.width as f64 / 2.0) as i32;
                let y = position.y as i32 + 5;
                let _ = window_click
                    .set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                let _ = window_click.show();
                let _ = window_click.set_focus();

                // With Accessory activation policy, macOS needs an explicit
                // activate call to bring the window to the foreground.
                #[cfg(target_os = "macos")]
                {
                    use objc2::{class, msg_send};
                    unsafe {
                        let app: *mut objc2::runtime::AnyObject =
                            msg_send![class!(NSApplication), sharedApplication];
                        let _: () = msg_send![app, activateIgnoringOtherApps: true];
                    }
                }
            }
        }
    });

    // Right-click menu events
    app.on_menu_event(|app, event| {
        if event.id().as_ref() == "quit" {
            app.exit(0);
        }
    });

    Ok(())
}
