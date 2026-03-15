/**
 * Config contract tests — ensure that ModelConfig objects constructed in the
 * frontend stay in sync with the Rust backend's expected shape.
 *
 * If you add a field to ModelConfig / TranscriptionConfig / SummarizationConfig
 * in types/index.ts, these tests will fail until you update:
 *   1. DEFAULT_CONFIG in Settings.tsx
 *   2. Test fixtures in App.test.tsx
 *   3. This contract test
 *
 * The `tsc --noEmit` step (npm run test:types) also catches this at compile
 * time, but these runtime tests give a clearer error message.
 */
import { describe, it, expect } from "vitest";
import type { ModelConfig, TranscriptionConfig, SummarizationConfig } from "../types";

// ─── Canonical config: must list EVERY field of ModelConfig ──────────────────
// When you add a field to the TS types, add it here too.  The test below will
// verify that no key is undefined.
const CANONICAL_CONFIG: ModelConfig = {
  captureMode: "mic_only",
  transcription: {
    provider: "local_file",
    modelPath: null,
    modelId: null,
    apiKey: null,
    language: null,
  },
  summarization: {
    enabled: true,
    provider: "ollama",
    baseUrl: "http://localhost:11434",
    apiKey: null,
    model: "llama3.2",
    prompt: null,
  },
  hotkeyToggle: null,
};

// ─── Expected keys per sub-config ────────────────────────────────────────────
// Keep these sorted alphabetically so diffs are easy to spot.
const TRANSCRIPTION_KEYS: (keyof TranscriptionConfig)[] = [
  "apiKey",
  "language",
  "modelId",
  "modelPath",
  "provider",
];

const SUMMARIZATION_KEYS: (keyof SummarizationConfig)[] = [
  "apiKey",
  "baseUrl",
  "enabled",
  "model",
  "prompt",
  "provider",
];

describe("ModelConfig contract", () => {
  it("CANONICAL_CONFIG has every TranscriptionConfig key", () => {
    const actual = Object.keys(CANONICAL_CONFIG.transcription).sort();
    expect(actual).toEqual([...TRANSCRIPTION_KEYS].sort());
  });

  it("CANONICAL_CONFIG has every SummarizationConfig key", () => {
    const actual = Object.keys(CANONICAL_CONFIG.summarization).sort();
    expect(actual).toEqual([...SUMMARIZATION_KEYS].sort());
  });

  it("no field in CANONICAL_CONFIG is undefined", () => {
    // undefined fields get stripped by JSON.stringify and may break serde
    const json = JSON.stringify(CANONICAL_CONFIG);
    const parsed = JSON.parse(json) as ModelConfig;

    for (const key of TRANSCRIPTION_KEYS) {
      expect(key in parsed.transcription).toBe(true);
    }
    for (const key of SUMMARIZATION_KEYS) {
      expect(key in parsed.summarization).toBe(true);
    }
  });

  it("round-trips through JSON without losing keys", () => {
    const json = JSON.stringify(CANONICAL_CONFIG);
    const parsed = JSON.parse(json) as ModelConfig;

    expect(Object.keys(parsed.transcription).sort()).toEqual(
      Object.keys(CANONICAL_CONFIG.transcription).sort()
    );
    expect(Object.keys(parsed.summarization).sort()).toEqual(
      Object.keys(CANONICAL_CONFIG.summarization).sort()
    );
  });
});
