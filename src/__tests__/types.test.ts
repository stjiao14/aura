import { describe, it, expect } from "vitest";
import { WHISPER_MODELS, PROVIDER_MODELS } from "../types";

describe("WHISPER_MODELS", () => {
  it("has at least one entry", () => {
    expect(WHISPER_MODELS.length).toBeGreaterThan(0);
  });

  it("first entry is multilingual (no .en in id)", () => {
    expect(WHISPER_MODELS[0].id).not.toContain(".en");
  });

  it("every model has id, label, and size", () => {
    for (const m of WHISPER_MODELS) {
      expect(m.id).toBeTruthy();
      expect(m.label).toBeTruthy();
      expect(m.size).toBeGreaterThan(0);
    }
  });

  it("all ids end in .bin", () => {
    for (const m of WHISPER_MODELS) {
      expect(m.id.endsWith(".bin")).toBe(true);
    }
  });
});

describe("PROVIDER_MODELS", () => {
  const expectedProviders = ["ollama", "openai", "anthropic", "gemini"];

  it("covers all cloud providers", () => {
    for (const p of expectedProviders) {
      expect(PROVIDER_MODELS[p]).toBeDefined();
      expect(PROVIDER_MODELS[p].length).toBeGreaterThan(0);
    }
  });

  it("anthropic models include a current claude-sonnet", () => {
    const models = PROVIDER_MODELS["anthropic"];
    expect(models.some((m) => m.includes("claude-sonnet"))).toBe(true);
  });
});
