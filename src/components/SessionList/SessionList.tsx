import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../../lib/ipc";
import type { Session } from "../../types";
import "./SessionList.css";

interface Props {
  onSelect: (session: Session) => void;
}

export default function SessionList({ onSelect }: Props) {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [processingStep, setProcessingStep] = useState<string | null>(null);

  const load = async () => setSessions(await ipc.listSessions());

  useEffect(() => {
    load();
    const unlistenUpdated = listen("session:updated", () => {
      load();
      setProcessingStep(null);
    });
    const unlistenStep = listen<string>("processing:step", (e) => {
      setProcessingStep(e.payload);
    });
    return () => {
      unlistenUpdated.then((f) => f());
      unlistenStep.then((f) => f());
    };
  }, []);

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    await ipc.deleteSession(id);
    setSessions((prev) => prev.filter((s) => s.id !== id));
  };

  if (sessions.length === 0) {
    return (
      <div className="session-empty">
        <p>No sessions yet.</p>
        <p className="session-empty-hint">Hit Record to start.</p>
      </div>
    );
  }

  return (
    <ul className="session-list">
      {sessions.map((s) => {
        return (
          <li
            key={s.id}
            className={`session-item ${s.status === "error" ? "has-error" : ""}`}
            onClick={() => onSelect(s)}
          >
            <div className="session-item-row">
              <span className="session-title">{s.title || "Untitled"}</span>
              <div className="session-item-right">
                <StatusBadge status={s.status} step={s.status === "processing" ? processingStep : null} />
                <button
                  className="session-delete-btn"
                  onClick={(e) => handleDelete(e, s.id)}
                  title="Delete session"
                >
                  ✕
                </button>
              </div>
            </div>
            <span className="session-meta">
              {new Date(s.startedAt).toLocaleString()}
              {s.durationSecs != null && ` · ${Math.round(s.durationSecs / 60)}m`}
            </span>
            {s.status === "error" && s.errorMessage && (
              <span className="session-error-msg">{s.errorMessage}</span>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function StatusBadge({ status, step }: { status: Session["status"]; step?: string | null }) {
  const label =
    status === "processing" && step === "transcribing" ? "Transcribing…" :
    status === "processing" && step === "summarizing"  ? "Summarizing…" :
    status === "recording"  ? "Recording…" :
    status === "processing" ? "Processing…" :
    status === "done"       ? "Done" : "Error";
  return <span className={`status-badge status-${status}`}>{label}</span>;
}
