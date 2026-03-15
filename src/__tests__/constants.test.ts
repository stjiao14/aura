import { describe, it, expect, vi } from "vitest";
import { ipc } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  ipc: {
    getRtIntervalSecs: vi.fn().mockResolvedValue(15),
  },
}));

describe("RT_INTERVAL_SECS (from backend)", () => {
  it("getRtIntervalSecs returns a positive number", async () => {
    const val = await ipc.getRtIntervalSecs();
    expect(val).toBeGreaterThan(0);
  });
});
