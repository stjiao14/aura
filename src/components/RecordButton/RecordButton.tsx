import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../../lib/ipc";
import type { Session } from "../../types";
import "./RecordButton.css";

export default function RecordButton({ disabled }: { disabled?: boolean }) {
  const [activeSession, setActiveSession] = useState<Session | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const [busy, setBusy] = useState(false);

  // Restore state on mount in case we were recording before this component mounted
  useEffect(() => {
    ipc.listSessions().then((sessions) => {
      const live = sessions.find((s) => s.status === "recording");
      if (live) setActiveSession(live);
    }).catch(() => {});
  }, []);

  useEffect(() => {
    const unlisten = listen<Session>("session:updated", (e) => {
      const s = e.payload;
      if (s.status === "recording") {
        setActiveSession(s);
      } else {
        setActiveSession(null);
        setElapsed(0);
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  useEffect(() => {
    if (!activeSession) return;
    const t = setInterval(() => setElapsed((s) => s + 1), 1000);
    return () => clearInterval(t);
  }, [activeSession]);

  const fmt = (s: number) =>
    `${Math.floor(s / 60).toString().padStart(2, "0")}:${(s % 60).toString().padStart(2, "0")}`;

  const handleClick = async () => {
    if (busy) return;
    setBusy(true);
    try {
      if (activeSession) {
        await ipc.stopSession(activeSession.id);
      } else {
        await ipc.startSession();
      }
    } catch (e: any) {
      alert(e.message || e);
    } finally {
      setBusy(false);
    }
  };

  const isRecording = !!activeSession;

  return (
    <div className="record-row">
      <button
        className={`record-btn ${isRecording ? "recording" : ""}`}
        onClick={handleClick}
        disabled={busy || disabled}
      >
        <span className="record-dot" />
        <span className="record-label">
          {isRecording ? "Stop" : "Start"}
        </span>
      </button>
      {isRecording && (
        <span className="record-timer">{fmt(elapsed)}</span>
      )}
    </div>
  );
}
