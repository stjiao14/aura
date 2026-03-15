import { useState } from "react";
import { ipc } from "../../lib/ipc";
import type { ModelConfig } from "../../types";
import { PROVIDER_MODELS } from "../../types";

interface Props {
  config: ModelConfig;
  updateSumm: (patch: Partial<ModelConfig["summarization"]>) => void;
  save: () => Promise<void>;
}

export default function SummarizationSettings({ config, updateSumm, save }: Props) {
  const [customModel, setCustomModel] = useState(() => {
    const presets = PROVIDER_MODELS[config.summarization.provider] ?? [];
    return !!config.summarization.model && !presets.includes(config.summarization.model);
  });
  const [testState, setTestState] = useState<"idle" | "testing" | "success" | "error">("idle");
  const [testError, setTestError] = useState<string | null>(null);

  const handleProviderChange = (provider: ModelConfig["summarization"]["provider"]) => {
    const presets = PROVIDER_MODELS[provider] ?? [];
    setCustomModel(false);
    updateSumm({ provider, model: presets[0] ?? null, apiKey: null, baseUrl: provider === "ollama" ? "http://localhost:11434" : null });
  };

  const presets = PROVIDER_MODELS[config.summarization.provider] ?? [];

  return (
    <section>
      <h3>Summarization (LLM)</h3>

      <label className="toggle-row">
        <span>
          Enable Summarization
          <span className="hint">Generate meeting notes after session ends</span>
        </span>
        <input
          type="checkbox"
          checked={config.summarization.enabled ?? true}
          onChange={(e) => updateSumm({ enabled: e.target.checked })}
        />
      </label>

      {config.summarization.enabled && (
        <>
          <label>
            Provider
        <select
          value={config.summarization.provider}
          onChange={(e) => handleProviderChange(e.target.value as ModelConfig["summarization"]["provider"])}
        >
          <option value="ollama">Ollama (local)</option>
          <option value="openai">OpenAI</option>
          <option value="anthropic">Anthropic</option>
          <option value="gemini">Gemini</option>
          <option value="local_file">Custom (OpenAI-compatible)</option>
        </select>
      </label>

      {(config.summarization.provider === "ollama" || config.summarization.provider === "local_file") && (
        <label>
          Base URL
          <input
            type="text"
            placeholder="http://localhost:11434"
            value={config.summarization.baseUrl ?? ""}
            onChange={(e) => updateSumm({ baseUrl: e.target.value || null })}
          />
        </label>
      )}

      {["openai", "anthropic", "gemini"].includes(config.summarization.provider) && (
        <label>
          API Key
          <input
            type="password"
            placeholder="Paste your API key"
            value={config.summarization.apiKey ?? ""}
            onChange={(e) => updateSumm({ apiKey: e.target.value || null })}
          />
        </label>
      )}

      {presets.length > 0 && (
        <label>
          Model
          <select
            value={customModel ? "__custom__" : (config.summarization.model ?? presets[0])}
            onChange={(e) => {
              if (e.target.value === "__custom__") {
                setCustomModel(true);
                updateSumm({ model: "" });
              } else {
                setCustomModel(false);
                updateSumm({ model: e.target.value });
              }
            }}
          >
            {presets.map((m) => <option key={m} value={m}>{m}</option>)}
            <option value="__custom__">Custom…</option>
          </select>
        </label>
      )}

      {(customModel || presets.length === 0) && (
        <label>
          Model name
          <input
            type="text"
            placeholder="e.g. llama3.2 / gpt-4o / claude-sonnet-4-6"
            value={config.summarization.model ?? ""}
            onChange={(e) => updateSumm({ model: e.target.value || null })}
          />
        </label>
      )}

      <label style={{ marginTop: 8 }}>
        Custom Prompt
        <span className="hint">Override the default summary prompt</span>
        <textarea
          className="chunk-edit-input"
          style={{ minHeight: "64px" }}
          placeholder="e.g. 'Format as bullet points with Action Items'"
          value={config.summarization.prompt ?? ""}
          onChange={(e) => updateSumm({ prompt: e.target.value || null })}
        />
      </label>
      </>
    )}

      <button
        className={`test-btn ${testState}`}
        onClick={async () => {
          setTestState("testing");
          setTestError(null);
          try {
            await save();
            await ipc.testProviderConnection();
            setTestState("success");
          } catch (e) {
            setTestError(String(e));
            setTestState("error");
          }
          setTimeout(() => setTestState("idle"), 4000);
        }}
        disabled={testState === "testing"}
      >
        {testState === "testing" ? "Testing…"
          : testState === "success" ? "Connected ✓"
          : testState === "error" ? "Failed ✗"
          : "Test Connection"}
      </button>
      {testState === "error" && testError && (
        <p className="test-error">{testError}</p>
      )}
    </section>
  );
}
