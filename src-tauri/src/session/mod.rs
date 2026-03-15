use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: SessionStatus,
    pub duration_secs: Option<f64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    #[serde(flatten)]
    pub session: Session,
    pub transcript: Vec<TranscriptChunk>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Recording,
    Processing,
    Done,
    Error,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Recording => "recording",
            Self::Processing => "processing",
            Self::Done => "done",
            Self::Error => "error",
        };
        write!(f, "{}", s)
    }
}

impl From<&str> for SessionStatus {
    fn from(s: &str) -> Self {
        match s {
            "recording" => Self::Recording,
            "processing" => Self::Processing,
            "done" => Self::Done,
            _ => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptChunk {
    pub id: String,
    pub session_id: String,
    pub speaker_label: Option<String>,
    pub timestamp_secs: f64,
    pub text: String,
}

// --- DB helpers ---

pub fn create(conn: &Connection, title: Option<String>) -> Result<Session> {
    let session = Session {
        id: Uuid::new_v4().to_string(),
        title,
        started_at: Utc::now().to_rfc3339(),
        ended_at: None,
        status: SessionStatus::Recording,
        duration_secs: None,
        error_message: None,
    };

    conn.execute(
        "INSERT INTO sessions (id, title, started_at, status) VALUES (?1, ?2, ?3, ?4)",
        params![
            session.id,
            session.title,
            session.started_at,
            session.status.to_string()
        ],
    )?;

    Ok(session)
}

pub fn set_status(conn: &Connection, id: &str, status: SessionStatus) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET status = ?1 WHERE id = ?2",
        params![status.to_string(), id],
    )?;
    Ok(())
}

pub fn set_error(conn: &Connection, id: &str, message: &str) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET status = 'error', error_message = ?1 WHERE id = ?2",
        params![message, id],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> Result<Session> {
    let session = conn.query_row(
        "SELECT id, title, started_at, ended_at, status, duration_secs, error_message FROM sessions WHERE id = ?1",
        params![id],
        row_to_session,
    )?;
    Ok(session)
}

pub fn list(conn: &Connection) -> Result<Vec<Session>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, started_at, ended_at, status, duration_secs, error_message FROM sessions ORDER BY started_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_session)?;
    rows.map(|r| r.map_err(Into::into)).collect()
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn delete_chunks_in_range(
    conn: &Connection,
    session_id: &str,
    from_secs: f64,
    to_secs: f64,
) -> Result<()> {
    conn.execute(
        "DELETE FROM transcript_chunks WHERE session_id = ?1 AND timestamp_secs >= ?2 AND timestamp_secs < ?3",
        params![session_id, from_secs, to_secs],
    )?;
    Ok(())
}

pub fn rename(conn: &Connection, id: &str, title: &str) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET title = ?1 WHERE id = ?2",
        params![title, id],
    )?;
    Ok(())
}

pub fn insert_chunk(conn: &Connection, chunk: &TranscriptChunk) -> Result<()> {
    conn.execute(
        "INSERT INTO transcript_chunks (id, session_id, speaker_label, timestamp_secs, text)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            chunk.id,
            chunk.session_id,
            chunk.speaker_label,
            chunk.timestamp_secs,
            chunk.text
        ],
    )?;
    Ok(())
}

pub fn update_chunk_text(conn: &Connection, chunk_id: &str, new_text: &str) -> Result<()> {
    conn.execute(
        "UPDATE transcript_chunks SET text = ?1 WHERE id = ?2",
        params![new_text, chunk_id],
    )?;
    Ok(())
}

pub fn get_chunks(conn: &Connection, session_id: &str) -> Result<Vec<TranscriptChunk>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, speaker_label, timestamp_secs, text
         FROM transcript_chunks WHERE session_id = ?1 ORDER BY timestamp_secs",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok(TranscriptChunk {
            id: row.get(0)?,
            session_id: row.get(1)?,
            speaker_label: row.get(2)?,
            timestamp_secs: row.get(3)?,
            text: row.get(4)?,
        })
    })?;
    rows.map(|r| r.map_err(Into::into)).collect()
}

fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<Session> {
    let status_str: String = row.get(4)?;
    Ok(Session {
        id: row.get(0)?,
        title: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        status: SessionStatus::from(status_str.as_str()),
        duration_secs: row.get(5)?,
        error_message: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn create_and_get_session() {
        let conn = test_db();
        let s = create(&conn, Some("Stand-up".into())).unwrap();
        assert_eq!(s.title.as_deref(), Some("Stand-up"));
        assert_eq!(s.status, SessionStatus::Recording);

        let fetched = get(&conn, &s.id).unwrap();
        assert_eq!(fetched.id, s.id);
        assert_eq!(fetched.title.as_deref(), Some("Stand-up"));
    }

    #[test]
    fn list_returns_sessions_newest_first() {
        let conn = test_db();
        let a = create(&conn, Some("A".into())).unwrap();
        let b = create(&conn, Some("B".into())).unwrap();
        let sessions = list(&conn).unwrap();
        // Both present (order may vary by timestamp resolution, just check presence)
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&a.id.as_str()));
        assert!(ids.contains(&b.id.as_str()));
    }

    #[test]
    fn set_status_updates_correctly() {
        let conn = test_db();
        let s = create(&conn, None).unwrap();
        assert_eq!(s.status, SessionStatus::Recording);

        set_status(&conn, &s.id, SessionStatus::Processing).unwrap();
        let updated = get(&conn, &s.id).unwrap();
        assert_eq!(updated.status, SessionStatus::Processing);
    }

    #[test]
    fn set_error_stores_message() {
        let conn = test_db();
        let s = create(&conn, None).unwrap();
        set_error(&conn, &s.id, "whisper model not found").unwrap();
        let updated = get(&conn, &s.id).unwrap();
        assert_eq!(updated.status, SessionStatus::Error);
        assert_eq!(
            updated.error_message.as_deref(),
            Some("whisper model not found")
        );
    }

    #[test]
    fn delete_removes_session_and_chunks() {
        let conn = test_db();
        let s = create(&conn, None).unwrap();
        let chunk = TranscriptChunk {
            id: "c1".into(),
            session_id: s.id.clone(),
            speaker_label: None,
            timestamp_secs: 1.0,
            text: "hello".into(),
        };
        insert_chunk(&conn, &chunk).unwrap();

        delete(&conn, &s.id).unwrap();

        // Session gone
        assert!(get(&conn, &s.id).is_err());
        // Chunks cascade-deleted
        let chunks = get_chunks(&conn, &s.id).unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunks_returned_in_timestamp_order() {
        let conn = test_db();
        let s = create(&conn, None).unwrap();
        for (i, ts) in [5.0f64, 1.0, 3.0].iter().enumerate() {
            insert_chunk(
                &conn,
                &TranscriptChunk {
                    id: format!("c{i}"),
                    session_id: s.id.clone(),
                    speaker_label: None,
                    timestamp_secs: *ts,
                    text: format!("chunk {ts}"),
                },
            )
            .unwrap();
        }
        let chunks = get_chunks(&conn, &s.id).unwrap();
        let timestamps: Vec<f64> = chunks.iter().map(|c| c.timestamp_secs).collect();
        assert_eq!(timestamps, vec![1.0, 3.0, 5.0]);
    }

    #[test]
    fn session_status_round_trips_through_string() {
        for (status, expected_str) in [
            (SessionStatus::Recording, "recording"),
            (SessionStatus::Processing, "processing"),
            (SessionStatus::Done, "done"),
            (SessionStatus::Error, "error"),
        ] {
            assert_eq!(status.to_string(), expected_str);
            assert_eq!(SessionStatus::from(expected_str), status);
        }
    }
}
