import "@testing-library/jest-dom";
import { vi } from "vitest";

// Mock the Tauri core invoke — tests control return values per-test with vi.mocked()
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

// Mock Tauri event bus — listen returns a promise resolving to a no-op unlistener
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// Mock dialog plugin (not used in component tests but imported transitively)
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

// Mock shell plugin (used by UpcomingMeetings for opening meeting URLs)
vi.mock("@tauri-apps/plugin-shell", () => ({
  open: vi.fn().mockResolvedValue(undefined),
}));
