import { useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { ModelConfig } from "../../types";
import AudioSettings from "./AudioSettings";
import TranscriptionSettings from "./TranscriptionSettings";
import SummarizationSettings from "./SummarizationSettings";
import "./Settings.css";

const DEFAULT_CONFIG: ModelConfig = {
  captureMode: "mic_only",
  transcription: { provider: "local_file", modelPath: null, modelId: null, apiKey: null, language: null },
  summarization: { enabled: true, provider: "ollama", baseUrl: "http://localhost:11434", apiKey: null, model: "llama3.2", prompt: null },
  hotkeyToggle: null,
};

export default function Settings({ onSaved }: { onSaved?: () => void }) {
  const [config, setConfig] = useState<ModelConfig>(DEFAULT_CONFIG);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    ipc.getModelConfig().then(setConfig).catch(() => {});
  }, []);

  const save = async () => {
    await ipc.setModelConfig(config);
    setSaved(true);
    onSaved?.();
    setTimeout(() => setSaved(false), 2000);
  };

  const updateSumm = (patch: Partial<ModelConfig["summarization"]>) =>
    setConfig((c) => ({ ...c, summarization: { ...c.summarization, ...patch } }));

  const updateTranscr = (patch: Partial<ModelConfig["transcription"]>) =>
    setConfig((c) => ({ ...c, transcription: { ...c.transcription, ...patch } }));

  return (
    <div className="settings">
      <AudioSettings config={config} setConfig={setConfig} />
      <TranscriptionSettings config={config} updateTranscr={updateTranscr} />
      <SummarizationSettings config={config} updateSumm={updateSumm} save={save} />

      <button className="save-btn" onClick={save}>
        {saved ? "Saved ✓" : "Save"}
      </button>
    </div>
  );
}
