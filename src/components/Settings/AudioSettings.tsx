import { useState, useEffect, useRef } from "react";
import { ipc } from "../../lib/ipc";
import type { ModelConfig } from "../../types";

const DEFAULT_HOTKEY = "CmdOrCtrl+Shift+R";

/** Format a Tauri shortcut string for macOS display (e.g. "CmdOrCtrl+Shift+R" → "⌘⇧R") */
function formatHotkey(hotkey: string): string {
  return hotkey
    .split("+")
    .map((part) => {
      switch (part) {
        case "CmdOrCtrl":
        case "Cmd":
          return "⌘";
        case "Ctrl":
          return "⌃";
        case "Shift":
          return "⇧";
        case "Alt":
          return "⌥";
        case "Space":
          return "Space";
        default:
          return part.toUpperCase();
      }
    })
    .join("");
}

/** Convert a KeyboardEvent to a Tauri shortcut string, or null if invalid (e.g. lone modifier). */
function eventToHotkey(e: KeyboardEvent): string | null {
  if (["Meta", "Control", "Shift", "Alt"].includes(e.key)) return null;

  const parts: string[] = [];
  if (e.metaKey || e.ctrlKey) parts.push("CmdOrCtrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");

  let key = e.key;
  if (key === " ") key = "Space";
  else if (key.length === 1) key = key.toUpperCase();

  // Must have at least one modifier
  if (parts.length === 0) return null;

  parts.push(key);
  return parts.join("+");
}

interface Props {
  config: ModelConfig;
  setConfig: React.Dispatch<React.SetStateAction<ModelConfig>>;
}

export default function AudioSettings({ config, setConfig }: Props) {
  const [capturing, setCapturing] = useState(false);
  const captureRef = useRef(capturing);
  captureRef.current = capturing;

  const currentHotkey = config.hotkeyToggle ?? DEFAULT_HOTKEY;

  useEffect(() => {
    if (!capturing) return;

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") {
        setCapturing(false);
        return;
      }
      const hotkey = eventToHotkey(e);
      if (!hotkey) return;

      const updated = { ...config, hotkeyToggle: hotkey };
      setConfig(updated);
      ipc.setModelConfig(updated).catch(console.error);
      setCapturing(false);
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturing, config, setConfig]);

  return (
    <section>
      <h3>Audio Sources</h3>

      <label className="toggle-row">
        <span>Microphone</span>
        <span className="badge always-on">Always on</span>
      </label>

      <label className="toggle-row">
        <span>
          System Audio
          <span className="hint">Captures far-end audio from calls & videos</span>
        </span>
        <input
          type="checkbox"
          checked={config.captureMode === "mic_and_system"}
          onChange={(e) => {
            const mode = e.target.checked ? "mic_and_system" : "mic_only";
            const updated = { ...config, captureMode: mode as ModelConfig["captureMode"] };
            setConfig(updated);
            ipc.setModelConfig(updated).catch((err) => {
              console.error("Failed to save capture mode:", err);
            });
          }}
        />
      </label>
      {config.captureMode === "mic_and_system" && (
        <p className="info-note">
          Aura will request <strong>Screen Recording</strong> permission — required by macOS for any system audio capture. No screen content is ever recorded or stored.
        </p>
      )}

      <div className="hotkey-row">
        <span>
          Global Hotkey
          <span className="hint">Toggle recording from anywhere</span>
        </span>
        <button
          className={`hotkey-btn${capturing ? " capturing" : ""}`}
          onClick={() => setCapturing(true)}
          title="Click then press a key combo"
        >
          {capturing ? "Press keys…" : formatHotkey(currentHotkey)}
        </button>
      </div>
    </section>
  );
}
