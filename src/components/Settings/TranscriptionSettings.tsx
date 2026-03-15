import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "../../lib/ipc";
import type { ModelConfig, TranscriptionConfig } from "../../types";
import { WHISPER_MODELS, WHISPER_LANGUAGES } from "../../types";

type DownloadState = "idle" | "downloading" | "done" | "error";

interface Props {
  config: ModelConfig;
  updateTranscr: (patch: Partial<ModelConfig["transcription"]>) => void;
}

export default function TranscriptionSettings({ config, updateTranscr }: Props) {
  const [dlState, setDlState] = useState<DownloadState>("idle");
  const [dlPct, setDlPct] = useState(0);
  const [dlError, setDlError] = useState<string | null>(null);

  useEffect(() => {
    if (config.transcription.provider === "hf_download") {
      const mid = config.transcription.modelId ?? WHISPER_MODELS[0].id;
      if (mid) ipc.checkModelDownloaded(mid).then((ok) => { if (ok) setDlState("done"); });
    }
  }, [config.transcription.provider]);

  useEffect(() => {
    const unlisten = listen<{ pct: number }>("model:download_progress", (e) => {
      setDlPct(Math.round(e.payload.pct * 100));
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const handleDownload = async () => {
    const modelId = config.transcription.modelId ?? WHISPER_MODELS[0].id;
    if (!modelId) return;
    setDlState("downloading");
    setDlPct(0);
    setDlError(null);
    try {
      await ipc.downloadModel(modelId);
      await ipc.setModelConfig({ ...config, transcription: { ...config.transcription, modelId } });
      setDlState("done");
    } catch (e) {
      setDlError(String(e));
      setDlState("error");
    }
  };

  return (
    <section>
      <h3>Transcription (Whisper)</h3>

      <label>
        Source
        <select
          value={config.transcription.provider}
          onChange={(e) => {
            const p = e.target.value as TranscriptionConfig["provider"];
            updateTranscr({
              provider: p,
              modelId: p === "hf_download" ? (config.transcription.modelId ?? WHISPER_MODELS[0].id) : config.transcription.modelId,
            });
          }}
        >
          <option value="whisper_api">OpenAI Whisper API (fastest setup)</option>
          <option value="hf_download">Download local model</option>
          <option value="local_file">Local model file</option>
        </select>
      </label>

      <label>
        Language
        <select
          value={config.transcription.language ?? "auto"}
          onChange={(e) => {
            const val = e.target.value;
            updateTranscr({ language: val === "auto" ? null : val });
          }}
        >
          {WHISPER_LANGUAGES.map((l) => (
            <option key={l.code} value={l.code}>{l.label}</option>
          ))}
        </select>
      </label>

      {config.transcription.provider === "whisper_api" && (
        <>
          <label>
            OpenAI API Key
            <input
              type="password"
              placeholder="sk-..."
              value={config.transcription.apiKey ?? ""}
              onChange={(e) => updateTranscr({ apiKey: e.target.value || null })}
            />
          </label>
          <p className="info-note">
            Uses OpenAI's cloud Whisper model. Best accuracy, no download needed — requires an <strong>OpenAI API key</strong>.
          </p>
        </>
      )}

      {config.transcription.provider === "local_file" && (
        <div className="file-picker-row">
          <span className="file-picker-label">Model file</span>
          <div className="file-picker-field">
            <span className="file-picker-path">
              {config.transcription.modelPath
                ? config.transcription.modelPath.split("/").pop()
                : "No file selected"}
            </span>
            <button
              className="file-picker-btn"
              onClick={async () => {
                const selected = await ipc.pickModelFile();
                if (selected) updateTranscr({ modelPath: selected });
              }}
            >
              Choose…
            </button>
          </div>
        </div>
      )}

      {config.transcription.provider === "hf_download" && (
        <div className="download-section">
          <label>
            Model
            <select
              value={config.transcription.modelId ?? WHISPER_MODELS[0].id}
              onChange={(e) => {
                const mid = e.target.value;
                updateTranscr({ modelId: mid });
                setDlState("idle");
                ipc.checkModelDownloaded(mid).then((ok) => { if (ok) setDlState("done"); });
              }}
            >
              {WHISPER_MODELS.map((m) => (
                <option key={m.id} value={m.id}>{m.label}</option>
              ))}
            </select>
          </label>

          {dlState === "idle" && (
            <button className="dl-btn" onClick={handleDownload}>
              Download
            </button>
          )}
          {dlState === "downloading" && (
            <div className="dl-progress">
              <div className="dl-progress-track">
                <div className="dl-bar" style={{ width: `${dlPct}%` }} />
              </div>
              <span style={{ fontSize: 12, color: "#8e8e93", flexShrink: 0 }}>{dlPct}%</span>
            </div>
          )}
          {dlState === "done" && (
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <p className="dl-done" style={{ margin: 0 }}>✓ Model ready to use</p>
              <button className="dl-btn" style={{ padding: "4px 12px", fontSize: 12 }} onClick={() => setDlState("idle")}>
                Re-download
              </button>
            </div>
          )}
          {dlState === "error" && (
            <p className="dl-error">Download failed: {dlError}</p>
          )}
        </div>
      )}
    </section>
  );
}
