import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import RecordButton from "../components/RecordButton/RecordButton";
import type { Session } from "../types";

const mockSession: Session = {
  id: "test-id-123",
  title: "Test Session",
  startedAt: new Date().toISOString(),
  endedAt: null,
  status: "recording",
  durationSecs: null,
  errorMessage: null,
};

beforeEach(() => {
  vi.mocked(invoke).mockResolvedValue([]);
});

describe("RecordButton", () => {
  it("shows Start button when idle", async () => {
    render(<RecordButton />);
    await waitFor(() => expect(screen.getByText("Start")).toBeInTheDocument());
  });

  it("is disabled when disabled prop is true", async () => {
    render(<RecordButton disabled />);
    await waitFor(() => {
      const btn = screen.getByRole("button");
      expect(btn).toBeDisabled();
    });
  });

  it("calls startSession when clicked while idle", async () => {
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "list_sessions") return Promise.resolve([]);
      if (cmd === "start_session") return Promise.resolve(mockSession);
      return Promise.resolve(null);
    });
    render(<RecordButton />);
    await waitFor(() => screen.getByText("Start"));
    fireEvent.click(screen.getByRole("button"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_session", expect.anything()));
  });

  it("restores recording state on mount if a session is already recording", async () => {
    vi.mocked(invoke).mockResolvedValue([mockSession]);
    render(<RecordButton />);
    await waitFor(() => expect(screen.getByText("Stop")).toBeInTheDocument());
  });

  it("shows elapsed timer when recording", async () => {
    vi.mocked(invoke).mockResolvedValue([mockSession]);
    render(<RecordButton />);
    await waitFor(() => expect(screen.getByText(/\d{2}:\d{2}/)).toBeInTheDocument());
  });
});
