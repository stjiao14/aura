import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { ipc } from "../../lib/ipc";
import type { CalendarEvent, Session } from "../../types";
import "./UpcomingMeetings.css";

interface Props {
  recordingSession?: Session | null;
  onViewRecording?: (session: Session) => void;
}

export default function UpcomingMeetings({ recordingSession, onViewRecording }: Props = {}) {
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [denied, setDenied] = useState(false);
  const [busy, setBusy] = useState<string | null>(null); // event title being joined

  const load = () => {
    ipc.listUpcomingEvents()
      .then((evts) => { setEvents(evts ?? []); setDenied(false); })
      .catch(() => setDenied(true));
  };

  useEffect(() => {
    load();
    const interval = setInterval(load, 5 * 60 * 1000); // refresh every 5 min
    return () => clearInterval(interval);
  }, []);

  const joinAndRecord = async (event: CalendarEvent) => {
    setBusy(event.title);
    try {
      const promises: Promise<unknown>[] = [ipc.startSession(event.title, event.attendees)];
      if (event.meetingUrl) {
        promises.push(open(event.meetingUrl));
      }
      await Promise.all(promises);
    } finally {
      setBusy(null);
    }
  };

  if (denied) {
    return (
      <div className="upcoming-denied">
        <span>Grant Calendar access in System Settings to see upcoming meetings.</span>
        <button 
          className="settings-link-btn"
          onClick={() => ipc.openSystemSettings("x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars")}
        >
          Open Settings →
        </button>
      </div>
    );
  }

  if (events.length === 0) return null; // no events = no section

  return (
    <div className="upcoming">
      <h3 className="upcoming-heading">Upcoming</h3>
      <div className="upcoming-list">
        {events.slice(0, 4).map((event, i) => {
          const isRecording = recordingSession?.title === event.title;
          return (
            <div key={i} className="upcoming-event">
              <div className="upcoming-info">
                <span className="upcoming-title">{event.title}</span>
                <span className="upcoming-time">
                  {formatTime(event.startDate)} – {formatTime(event.endDate)}
                </span>
              </div>
              {isRecording && recordingSession ? (
                <button
                  className="upcoming-join-btn recording"
                  onClick={() => onViewRecording?.(recordingSession)}
                >
                  View Recording
                </button>
              ) : (
                <button
                  className="upcoming-join-btn"
                  onClick={() => joinAndRecord(event)}
                  disabled={busy !== null}
                >
                  {busy === event.title
                    ? "Joining…"
                    : event.meetingUrl
                      ? "Join & Record"
                      : "Record"}
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}
