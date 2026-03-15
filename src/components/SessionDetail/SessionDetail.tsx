import { useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../../lib/ipc";
import type { Session, SessionDetail as SessionDetailType } from "../../types";
import "./SessionDetail.css";

interface Props {
  session: Session;
  onBack: () => void;
  onStop?: () => void;
}

export default function SessionDetail({ session, onBack, onStop }: Props) {
  const [detail, setDetail] = useState<SessionDetailType | null>(null);
  const isLive = session.status === "recording" || session.status === "processing";
  const [tab, setTab] = useState<"notes" | "transcript">(isLive ? "transcript" : "notes");

  // Editable title
  const [title, setTitle] = useState(session.title ?? "");
  const [editingTitle, setEditingTitle] = useState(false);
  const titleInputRef = useRef<HTMLInputElement>(null);

  // Notes editing
  const [editingNotes, setEditingNotes] = useState(false);
  const [notesText, setNotesText] = useState<string>("");
  // Streaming notes — tokens accumulate here during LLM generation
  const [streamingNotes, setStreamingNotes] = useState("");

  // Transcript editing & scrolling
  const [editingChunkId, setEditingChunkId] = useState<string | null>(null);
  const [editingChunkText, setEditingChunkText] = useState("");
  const transcriptEndRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [rtInterval, setRtInterval] = useState(15);

  // Follow mode: auto-scroll when user is at the bottom, disengage when they scroll up.
  const [followMode, setFollowMode] = useState(true);
  const prevChunkCount = useRef(0);

  const load = () => ipc.getSession(session.id).then(setDetail).catch(() => {});

  useEffect(() => {
    ipc.getRtIntervalSecs().then(setRtInterval).catch(() => {});
  }, []);

  // Detect user scroll to toggle follow mode.
  // The actual scroll container is the parent .app-main element.
  useEffect(() => {
    const el = scrollContainerRef.current?.closest(".app-main") as HTMLElement | null;
    if (!el) return;
    const onScroll = () => {
      const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      setFollowMode(atBottom);
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  // Auto-scroll only in follow mode and only when new chunks arrive (not after edits)
  useEffect(() => {
    const count = detail?.transcript?.length ?? 0;
    const isNewChunk = count > prevChunkCount.current;
    prevChunkCount.current = count;

    if (tab === "transcript" && followMode && isNewChunk) {
      transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [detail?.transcript?.length, tab, followMode]);

  useEffect(() => { load(); }, [session.id]);

  useEffect(() => {
    if (detail && !editingNotes) setNotesText(detail.notes ?? "");
  }, [detail?.notes]);

  useEffect(() => {
    if (!isLive) return;
    const unlisten = listen("session:updated", () => {
      load();
      setStreamingNotes(""); // final clean version arriving via load()
    });
    return () => { unlisten.then((f) => f()); };
  }, [session.id, isLive]);

  // Stream notes tokens during LLM generation (processing state only)
  useEffect(() => {
    if (session.status !== "processing") return;
    const unlisten = listen<string>("notes:chunk", (e) => {
      setStreamingNotes((prev) => prev + e.payload);
      setTab("notes");
    });
    return () => { unlisten.then((f) => f()); };
  }, [session.id, session.status]);

  useEffect(() => {
    if (editingTitle) titleInputRef.current?.focus();
  }, [editingTitle]);

  const commitNotes = async () => {
    setEditingNotes(false);
    await ipc.updateSessionNotes(session.id, notesText);
    load();
  };

  const commitTitle = async () => {
    setEditingTitle(false);
    const trimmed = title.trim();
    if (trimmed && trimmed !== (session.title ?? "")) {
      await ipc.renameSession(session.id, trimmed);
    }
  };

  const startEditChunk = (chunkId: string, text: string) => {
    setEditingChunkId(chunkId);
    setEditingChunkText(text);
  };

  const commitChunk = async (chunkId: string) => {
    const trimmed = editingChunkText.trim();
    setEditingChunkId(null);
    if (!trimmed) return;
    
    // Optimistic update
    setDetail((prev) => {
      if (!prev) return prev;
      return {
        ...prev,
        transcript: prev.transcript.map(c => c.id === chunkId ? { ...c, text: trimmed } : c)
      };
    });
    
    await ipc.updateTranscriptChunk(chunkId, trimmed);
    load(); // Refresh full state just in case
  };

  if (!detail) return <div className="detail-loading">Loading…</div>;

  const scrollToBottom = () => {
    transcriptEndRef.current?.scrollIntoView({ behavior: "smooth" });
    setFollowMode(true);
  };

  return (
    <div className="session-detail" ref={scrollContainerRef}>
      <div className="detail-toolbar">
        <div className="detail-header">
          <button className="back-btn" onClick={onBack}>←</button>

          {editingTitle ? (
            <input
              ref={titleInputRef}
              className="title-input"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              onBlur={commitTitle}
              onKeyDown={(e) => { if (e.key === "Enter") commitTitle(); if (e.key === "Escape") { setTitle(session.title ?? ""); setEditingTitle(false); } }}
            />
          ) : (
            <h2
              className="detail-title"
              title="Click to rename"
              onClick={() => setEditingTitle(true)}
            >
              {title || "Untitled"}
              <span className="edit-hint">✎</span>
            </h2>
          )}

          {onStop && (
            <button className="stop-btn-inline" onClick={onStop}>■ Stop</button>
          )}
        </div>

        <p className="detail-meta">{new Date(detail.startedAt).toLocaleString()}</p>

        <div className="detail-tabs">
          <button className={tab === "notes" ? "active" : ""} onClick={() => setTab("notes")}>Notes</button>
          <button className={tab === "transcript" ? "active" : ""} onClick={() => setTab("transcript")}>
            Transcript
            {isLive && detail.transcript.length > 0 && <span className="live-dot" />}
          </button>
        </div>
      </div>

      <div className="detail-body">
        {tab === "notes" ? (
          session.status === "recording" ? (
            <p className="notes-pending">Notes will be generated after recording stops.</p>
          ) : streamingNotes ? (
            <div className="notes-content notes-streaming">
              <ReactMarkdown>{streamingNotes}</ReactMarkdown>
            </div>
          ) : editingNotes ? (
            <textarea
              className="notes-edit-input"
              autoFocus
              value={notesText}
              onChange={(e) => setNotesText(e.target.value)}
              onBlur={commitNotes}
              onKeyDown={(e) => { if (e.key === "Escape") { setNotesText(detail.notes ?? ""); setEditingNotes(false); } }}
            />
          ) : (
            <div
              className="notes-content"
              title="Click to edit"
              onClick={() => { setNotesText(detail.notes ?? ""); setEditingNotes(true); }}
            >
              <ReactMarkdown>{detail.notes ?? "No notes generated."}</ReactMarkdown>
            </div>
          )
        ) : (
          <div className="transcript-content">
            {detail.transcript.length === 0 ? (
              <p className="transcript-empty">
                {isLive ? `Listening… transcript appears every ~${rtInterval} seconds.` : "No transcript."}
              </p>
            ) : (
              <>
                {detail.transcript.map((chunk) => (
                  <div key={chunk.id} className="chunk" onDoubleClick={() => startEditChunk(chunk.id, chunk.text)}>
                    <span className="chunk-time">
                      {chunk.speakerLabel ? `${chunk.speakerLabel} ` : ""}
                      [{formatTime(chunk.timestampSecs)}]
                    </span>
                    {editingChunkId === chunk.id ? (
                      <textarea
                        className="chunk-edit-input"
                        autoFocus
                        value={editingChunkText}
                        onChange={(e) => setEditingChunkText(e.target.value)}
                        onBlur={() => commitChunk(chunk.id)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && !e.shiftKey) {
                            e.preventDefault();
                            commitChunk(chunk.id);
                          }
                          if (e.key === "Escape") {
                            setEditingChunkId(null);
                          }
                        }}
                      />
                    ) : (
                      <p className="chunk-text" title="Double click to edit">{chunk.text}</p>
                    )}
                  </div>
                ))}
                <div ref={transcriptEndRef} className="transcript-bottom-anchor" />
              </>
            )}
          </div>
        )}
      </div>

      {isLive && !followMode && tab === "transcript" && (
        <button className="scroll-to-bottom" onClick={scrollToBottom}>
          ↓ Latest
        </button>
      )}
    </div>
  );
}

function formatTime(secs: number): string {
  const m = Math.floor(secs / 60).toString().padStart(2, "0");
  const s = Math.floor(secs % 60).toString().padStart(2, "0");
  return `${m}:${s}`;
}
