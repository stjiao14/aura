use crate::audio;
use crate::commands::models::load_model_config;
use crate::commands::recorder::spawn_rt_loop;
use crate::commands::{safe_lock, FriendlyError};
use crate::models::CaptureMode;
use crate::session::{self, Session, SessionDetail, SessionStatus};
use crate::storage::{self, AppDataDir};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

pub(crate) struct ProgressiveState {
    pub samples: Vec<f32>,
    pub start_offset: f64,
    pub prev_tail: Vec<f32>,
    pub attendee_info: Option<String>,
}

pub(crate) struct ActiveSession {
    pub handle: audio::CaptureHandle,
    pub loop_task: tokio::task::JoinHandle<()>,
    pub vad_task: tokio::task::JoinHandle<()>,
    pub progressive_state: std::sync::Arc<std::sync::Mutex<ProgressiveState>>,
}

pub struct ActiveSessions(pub Mutex<HashMap<String, ActiveSession>>);

// ─── Recording ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_session(
    title: Option<String>,
    attendees: Option<Vec<String>>,
    app: AppHandle,
    data_dir: State<'_, AppDataDir>,
) -> Result<Session, String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    let config = load_model_config(&conn).friendly()?;
    let system_audio = config.capture_mode == CaptureMode::MicAndSystem;

    crate::commands::models::validate_config_with_data_dir(&config, Some(&data_dir.0))
        .map_err(|e| crate::commands::recorder::friendly_error(&e.to_string()))?;

    let sess = session::create(&conn, title).friendly()?;

    // Stop any existing active sessions before starting the new one.
    // Do this BEFORE setting SUPPRESS_AUTOHIDE, because stop_session_internal
    // clears the flag — and we need it to stay true for the new session.
    let active_ids: Vec<String> = {
        let active_state = app.state::<ActiveSessions>();
        let active = safe_lock(&active_state.0)?;
        active.keys().cloned().collect()
    };
    for old_id in active_ids {
        tracing::info!("Auto-stopping previous session: {}", old_id);
        let _ = stop_session_internal(&old_id, &app, &data_dir.0).await;
    }

    crate::SUPPRESS_AUTOHIDE.store(true, Ordering::SeqCst);

    let handle = match audio::start_capture(system_audio) {
        Ok(h) => h,
        Err(e) => {
            crate::SUPPRESS_AUTOHIDE.store(false, Ordering::SeqCst);
            let _ = session::delete(&conn, &sess.id);
            return Err(e.to_string());
        }
    };

    let attendee_info = attendees.and_then(|a| {
        if a.is_empty() {
            None
        } else {
            Some(a.join(", "))
        }
    });

    let progressive_state = std::sync::Arc::new(std::sync::Mutex::new(ProgressiveState {
        samples: vec![],
        start_offset: 0.0,
        prev_tail: vec![],
        attendee_info,
    }));

    let (sentence_tx, sentence_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<f32>>();

    let vad_task = crate::commands::recorder::spawn_vad_drain(
        app.clone(),
        data_dir.0.clone(),
        sess.id.clone(),
        sentence_tx,
    );

    let loop_task = spawn_rt_loop(
        app.clone(),
        data_dir.0.clone(),
        sess.id.clone(),
        progressive_state.clone(),
        sentence_rx,
    );

    safe_lock(&app.state::<ActiveSessions>().0)?.insert(
        sess.id.clone(),
        ActiveSession {
            handle,
            loop_task,
            vad_task,
            progressive_state,
        },
    );

    app.emit("session:updated", &sess).ok();
    Ok(sess)
}

#[tauri::command]
pub async fn stop_session(
    id: String,
    app: AppHandle,
    data_dir: State<'_, AppDataDir>,
) -> Result<Session, String> {
    stop_session_internal(&id, &app, &data_dir.0).await
}

pub(crate) async fn stop_session_internal(
    id: &str,
    app: &AppHandle,
    data_dir_path: &std::path::Path,
) -> Result<Session, String> {
    crate::SUPPRESS_AUTOHIDE.store(false, Ordering::SeqCst);

    let active = safe_lock(&app.state::<ActiveSessions>().0)?.remove(id);

    let (whisper_samples, prog_start, prog_overlap_len, attendee_info) = match active {
        Some(a) => {
            a.vad_task.abort();
            let _ = a.vad_task.await;
            a.loop_task.abort();
            let _ = a.loop_task.await;
            let final_drain = a.handle.stop();

            let mut pstate = safe_lock(&a.progressive_state)?;
            pstate.samples.extend_from_slice(&final_drain);

            let mut input = pstate.prev_tail.clone();
            let overlap_len = input.len();
            input.extend_from_slice(&pstate.samples);

            let attendee_info = pstate.attendee_info.clone();

            (input, pstate.start_offset, overlap_len, attendee_info)
        }
        None => (vec![], 0.0, 0, None),
    };

    let conn = storage::open(data_dir_path).friendly()?;
    session::set_status(&conn, id, SessionStatus::Processing).friendly()?;
    let sess = session::get(&conn, id).friendly()?;
    app.emit("session:updated", &sess).ok();

    let app2 = app.clone();
    let data_dir_path_clone = data_dir_path.to_path_buf();
    let session_id = id.to_string();

    tokio::spawn(async move {
        if let Err(e) = crate::commands::recorder::run_post_processing(
            &app2,
            &data_dir_path_clone,
            &session_id,
            whisper_samples,
            prog_start,
            prog_overlap_len,
            attendee_info,
        )
        .await
        {
            tracing::error!("Post-processing failed for {}: {}", session_id, e);
            if let Ok(conn) = storage::open(&data_dir_path_clone) {
                let msg = crate::commands::recorder::friendly_error(&e.to_string());
                let _ = session::set_error(&conn, &session_id, &msg);
                if let Ok(s) = session::get(&conn, &session_id) {
                    app2.emit("session:updated", &s).ok();
                }
            }
        }
    });

    Ok(sess)
}

// ─── Session CRUD ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_sessions(data_dir: State<'_, AppDataDir>) -> Result<Vec<Session>, String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    session::list(&conn).friendly()
}

#[tauri::command]
pub fn get_session(id: String, data_dir: State<'_, AppDataDir>) -> Result<SessionDetail, String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    let s = session::get(&conn, &id).friendly()?;
    let chunks = session::get_chunks(&conn, &id).friendly()?;

    let notes = conn
        .query_row(
            "SELECT notes_path FROM sessions WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
        .and_then(|p| std::fs::read_to_string(p).ok());

    Ok(SessionDetail {
        session: s,
        transcript: chunks,
        notes,
    })
}

#[tauri::command]
pub fn rename_session(
    id: String,
    title: String,
    data_dir: State<'_, AppDataDir>,
) -> Result<(), String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    session::rename(&conn, &id, &title).friendly()
}

#[tauri::command]
pub fn update_transcript_chunk(
    id: String,
    text: String,
    data_dir: State<'_, AppDataDir>,
) -> Result<(), String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    session::update_chunk_text(&conn, &id, &text).friendly()
}

#[tauri::command]
pub fn update_session_notes(
    id: String,
    notes: String,
    data_dir: State<'_, AppDataDir>,
) -> Result<(), String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    let notes_path: Option<String> = conn
        .query_row(
            "SELECT notes_path FROM sessions WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .friendly()?;

    let path = notes_path.ok_or_else(|| "Session has no notes file yet".to_string())?;
    std::fs::write(&path, &notes).friendly()
}

#[tauri::command]
pub async fn delete_session(
    id: String,
    app: AppHandle,
    data_dir: State<'_, AppDataDir>,
) -> Result<(), String> {
    let active = safe_lock(&app.state::<ActiveSessions>().0)?.remove(&id);
    if let Some(a) = active {
        a.vad_task.abort();
        let _ = a.vad_task.await;
        a.loop_task.abort();
        let _ = a.loop_task.await;
        a.handle.stop();
        tracing::info!("Stopped active recording for deleted session {}", id);

        let conn = storage::open(&data_dir.0).friendly()?;
        if let Ok(mut s) = session::get(&conn, &id) {
            s.status = crate::session::SessionStatus::Done;
            app.emit("session:updated", &s).ok();
        }
    }
    let conn = storage::open(&data_dir.0).friendly()?;
    session::delete(&conn, &id).friendly()
}

#[tauri::command]
pub fn export_session(
    id: String,
    dest_dir: String,
    data_dir: State<'_, AppDataDir>,
) -> Result<String, String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    let s = session::get(&conn, &id).friendly()?;
    let chunks = session::get_chunks(&conn, &id).friendly()?;

    let title = s.title.as_deref().unwrap_or("untitled");
    let safe: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let out = std::path::Path::new(&dest_dir).join(format!("{}_{}.md", safe, &id[..8]));

    let mut md = format!("# {}\n\nDate: {}\n\n", title, s.started_at);
    for c in &chunks {
        md.push_str(&format!(
            "**{}** [{:.0}s]: {}\n\n",
            c.speaker_label.as_deref().unwrap_or("Speaker"),
            c.timestamp_secs,
            c.text
        ));
    }

    std::fs::write(&out, md).friendly()?;
    Ok(out.to_string_lossy().to_string())
}
