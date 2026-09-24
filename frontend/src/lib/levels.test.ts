import { describe, expect, it } from "vitest";
import { peakToMeter, smoothMeter } from "./levels";

describe("peakToMeter (#113)", () => {
  it("maps -60..0 dBFS onto 0..1", () => {
    expect(peakToMeter(0)).toBe(0);
    expect(peakToMeter(0.001)).toBeCloseTo(0, 5); // -60 dBFS
    expect(peakToMeter(0.01)).toBeCloseTo(1 / 3, 5); // -40 dBFS
    expect(peakToMeter(1)).toBe(1);
    expect(peakToMeter(4)).toBe(1);
  });
  it("treats NaN and negative input as silence", () => {
    expect(peakToMeter(Number.NaN)).toBe(0);
    expect(peakToMeter(-0.5)).toBe(0);
  });
});

describe("smoothMeter (#113)", () => {
  it("rises at once and falls slowly", () => {
    expect(smoothMeter(0.2, 0.8)).toBe(0.8);
    expect(smoothMeter(0.8, 0)).toBeCloseTo(0.74, 5);
    expect(smoothMeter(0.03, 0)).toBe(0);
  });
});
