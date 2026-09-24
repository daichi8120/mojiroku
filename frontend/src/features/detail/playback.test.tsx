import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { markMatches } from "@/components/composite";
import { nextRate, PLAYBACK_RATES } from "./AudioPlayer";

describe("nextRate (#111)", () => {
  it("cycles through the speeds and wraps", () => {
    expect(PLAYBACK_RATES.map((r) => nextRate(r))).toEqual([1.25, 1.5, 2, 1]);
  });
  it("falls back to the first speed from an unknown value", () => {
    expect(nextRate(3)).toBe(1);
  });
});

describe("markMatches (#111)", () => {
  const html = (text: string, q: string) => renderToStaticMarkup(<p>{markMatches(text, q)}</p>);
  it("marks every match, ignoring case", () => {
    expect(html("Test the test", "test")).toBe("<p><mark class=\"rounded-sm bg-amber/30 px-px text-ink\">Test</mark> the <mark class=\"rounded-sm bg-amber/30 px-px text-ink\">test</mark></p>");
  });
  it("leaves text alone for an empty query", () => {
    expect(html("リリース範囲", "  ")).toBe("<p>リリース範囲</p>");
  });
  it("works for Japanese", () => {
    expect(html("今週のリリース範囲", "リリース")).toContain("<mark");
  });
});
