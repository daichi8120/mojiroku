import { beforeEach, describe, expect, it } from "vitest";
import { getDiarizePref, setDiarizePref } from "./prefs";

describe("diarize preference (#115)", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    (globalThis as { localStorage?: unknown }).localStorage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    };
  });
  it("is on by default and remembers the last choice", () => {
    expect(getDiarizePref()).toBe(true);
    setDiarizePref(false);
    expect(getDiarizePref()).toBe(false);
    setDiarizePref(true);
    expect(getDiarizePref()).toBe(true);
  });
  it("falls back to on when storage is unavailable", () => {
    (globalThis as { localStorage?: unknown }).localStorage = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(getDiarizePref()).toBe(true);
    expect(() => setDiarizePref(false)).not.toThrow();
  });
});
