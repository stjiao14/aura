import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import RecordButton from "./components/RecordButton/RecordButton";
import SessionList from "./components/SessionList/SessionList";
import UpcomingMeetings from "./components/UpcomingMeetings/UpcomingMeetings";
import SessionDetail from "./components/SessionDetail/SessionDetail";
import Settings from "./components/Settings/Settings";
import { ipc } from "./lib/ipc";
import type { Session } from "./types";
import "./App.css";

type View = "home" | "settings";

export default function App() {
  const [view, setView] = useState<View>("home");
  const [selectedSession, setSelectedSession] = useState<Session | null>(null);
  // Tracks the in-progress recording session independently of explicit navigation.
  // Survives view switches so the live detail is always accessible.
  const [recordingSession, setRecordingSession] = useState<Session | null>(null);
  // True when user explicitly clicks Back during a live session — stops auto-showing the live detail.
  const [liveViewDismissed, setLiveViewDismissed] = useState(false);
  const [needsSetup, setNeedsSetup] = useState(false);

  useEffect(() => {
    ipc.getModelConfig().then((c) => {
      const hasTranscription =
        (c.transcription.provider === "local_file" && !!c.transcription.modelPath) ||
        (c.transcription.provider === "hf_download" && !!c.transcription.modelId) ||
        (c.transcription.provider === "whisper_api" && !!c.transcription.apiKey);
      setNeedsSetup(!hasTranscription);
    }).catch(() => setNeedsSetup(true));

    // Restore recording session if one was already in progress on mount
    ipc.listSessions().then((sessions) => {
      const live = sessions.find((s) => s.status === "recording");
      if (live) { setRecordingSession(live); setView("home"); }
    }).catch(() => {});
  }, []);

  // Global hotkey toggle: Cmd+Shift+R
  useEffect(() => {
    const unlisten = listen("hotkey:toggle", () => {
      if (recordingSession) {
        ipc.stopSession(recordingSession.id).catch(() => {});
      } else if (!needsSetup) {
        ipc.startSession().catch(() => {});
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, [recordingSession, needsSetup]);

  // Track the active recording session across all view switches
  useEffect(() => {
    const unlisten = listen<Session>("session:updated", (e) => {
      const s = e.payload;
      if (s.status === "recording") {
        setRecordingSession(s);
        setLiveViewDismissed(false); // new recording always opens the live view
        setView("home");
        setSelectedSession(null);
      } else {
        setRecordingSession((prev) => (prev?.id === s.id ? null : prev));
        setLiveViewDismissed(false);
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const goSettings = () => { setView("settings"); setSelectedSession(null); };

  // What to show in the main area: explicit user selection > live recording (unless dismissed) > list
  const activeSession = selectedSession ?? (liveViewDismissed ? null : recordingSession);

  return (
    <div className="app">
      <header className="app-header">
        <span className="app-title">Aura</span>
        <nav>
          <button
            className={view === "home" && !activeSession ? "active" : ""}
            onClick={() => { setView("home"); setSelectedSession(null); }}
          >
            Sessions
          </button>
          <button
            className={view === "settings" ? "active" : ""}
            onClick={goSettings}
          >
            Settings
          </button>
        </nav>
      </header>

      {needsSetup && view === "home" && !activeSession && (
        <div className="setup-banner">
          <span>Set up a transcription model to start recording.</span>
          <button onClick={goSettings}>Configure →</button>
        </div>
      )}

      <main className="app-main">
        {view === "settings" ? (
          <Settings onSaved={() => setNeedsSetup(false)} />
        ) : activeSession ? (
          <SessionDetail
            session={activeSession}
            onBack={() => {
              if (activeSession.id === recordingSession?.id) {
                setLiveViewDismissed(true); // user dismissed live view; show list + stop button
              }
              setSelectedSession(null);
            }}
            onStop={
              activeSession.status === "recording"
                ? () => ipc.stopSession(activeSession.id)
                : undefined
            }
          />
        ) : (
          <>
            <RecordButton disabled={needsSetup} />
            <UpcomingMeetings
              recordingSession={recordingSession}
              onViewRecording={(s) => setSelectedSession(s)}
            />
            <SessionList onSelect={setSelectedSession} />
          </>
        )}
      </main>
    </div>
  );
}
