import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import App from "../App";
import type { ModelConfig } from "../types";

const configuredConfig: ModelConfig = {
  captureMode: "mic_only",
  transcription: { provider: "whisper_api", modelPath: null, modelId: null, apiKey: "sk-test", language: null },
  summarization: { enabled: true, provider: "ollama", baseUrl: "http://localhost:11434", apiKey: null, model: "llama3.2", prompt: null },
  hotkeyToggle: null,
};

const unconfiguredConfig: ModelConfig = {
  captureMode: "mic_only",
  transcription: { provider: "local_file", modelPath: null, modelId: null, apiKey: null, language: null },
  summarization: { enabled: true, provider: "ollama", baseUrl: null, apiKey: null, model: null, prompt: null },
  hotkeyToggle: null,
};

beforeEach(() => {
  vi.mocked(invoke).mockImplementation((cmd) => {
    if (cmd === "get_model_config") return Promise.resolve(configuredConfig);
    if (cmd === "list_sessions") return Promise.resolve([]);
    return Promise.resolve(null);
  });
});

describe("App", () => {
  it("renders the Sessions and Settings nav buttons", async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText("Sessions")).toBeInTheDocument();
      expect(screen.getByText("Settings")).toBeInTheDocument();
    });
  });

  it("shows setup banner when transcription is not configured", async () => {
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_model_config") return Promise.resolve(unconfiguredConfig);
      if (cmd === "list_sessions") return Promise.resolve([]);
      return Promise.resolve(null);
    });
    render(<App />);
    await waitFor(() =>
      expect(screen.getByText(/Set up a transcription model/)).toBeInTheDocument()
    );
  });

  it("hides setup banner when transcription is configured", async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.queryByText(/Set up a transcription model/)).not.toBeInTheDocument();
    });
  });

  it("navigates to Settings view when Settings is clicked", async () => {
    render(<App />);
    await waitFor(() => screen.getByText("Settings"));
    fireEvent.click(screen.getByText("Settings"));
    await waitFor(() => expect(screen.getByText(/Transcription/)).toBeInTheDocument());
  });

  it("navigates back to Sessions view", async () => {
    render(<App />);
    await waitFor(() => screen.getByText("Settings"));
    fireEvent.click(screen.getByText("Settings"));
    await waitFor(() => screen.getByText(/Transcription/));
    fireEvent.click(screen.getByText("Sessions"));
    await waitFor(() => expect(screen.getByText("Start")).toBeInTheDocument());
  });
});
