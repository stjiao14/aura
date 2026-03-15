use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub title: String,
    pub start_date: String,
    pub end_date: String,
    pub meeting_url: Option<String>,
    pub attendees: Vec<String>,
    pub calendar_name: String,
}

extern "C" {
    fn aura_calendar_events() -> *const std::ffi::c_char;
    fn aura_calendar_events_free(ptr: *mut std::ffi::c_char);
}

/// Fetch upcoming calendar events (next 24 hours) from macOS EventKit.
/// Blocks on a semaphore internally — call via `spawn_blocking`.
pub fn list_upcoming_events() -> Result<Vec<CalendarEvent>, String> {
    let json_ptr = unsafe { aura_calendar_events() };
    if json_ptr.is_null() {
        return Err("Calendar access denied. Grant permission in System Settings → Privacy & Security → Calendars.".into());
    }

    let c_str = unsafe { std::ffi::CStr::from_ptr(json_ptr) };
    let json_str = c_str.to_str().map_err(|e| e.to_string())?;
    let events: Vec<CalendarEvent> = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse calendar data: {e}"))?;

    unsafe { aura_calendar_events_free(json_ptr as *mut _) };
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_event_serializes_to_camel_case() {
        let event = CalendarEvent {
            title: "Stand-up".into(),
            start_date: "2026-03-13T10:00:00Z".into(),
            end_date: "2026-03-13T10:30:00Z".into(),
            meeting_url: Some("https://meet.google.com/abc-defg-hij".into()),
            attendees: vec!["Alice".into(), "Bob".into()],
            calendar_name: "Work".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("startDate").is_some());
        assert!(json.get("meetingUrl").is_some());
        assert!(json.get("calendarName").is_some());
        // Must not have snake_case keys
        assert!(json.get("start_date").is_none());
        assert!(json.get("meeting_url").is_none());
    }

    #[test]
    fn calendar_event_round_trips() {
        let original = CalendarEvent {
            title: "1:1".into(),
            start_date: "2026-03-13T14:00:00Z".into(),
            end_date: "2026-03-13T14:30:00Z".into(),
            meeting_url: None,
            attendees: vec![],
            calendar_name: "Personal".into(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: CalendarEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.title, "1:1");
        assert!(parsed.meeting_url.is_none());
        assert!(parsed.attendees.is_empty());
    }
}
