import { describe, expect, it } from "vitest";
import { findSummary, templateLabel } from "./templates";
import type { Summary } from "./types";

describe("templateLabel", () => {
  // ⚠️ この期待値は Rust 側 export/common.rs の template_label テストと突き合わせて固定する。
  // ズレると Notion/Slack 追記との整合が壊れる（両言語とも一致必須）。
  it("既知 id を出力ラベルへ（ja。Rust の export::template_label と一致）", () => {
    expect(templateLabel("minutes", "ja")).toBe("議事録");
    expect(templateLabel("summary", "ja")).toBe("要約");
    expect(templateLabel("action_items", "ja")).toBe("アクションアイテム");
  });

  it("既知 id を出力ラベルへ（en。Rust の export::template_label と一致）", () => {
    expect(templateLabel("minutes", "en")).toBe("Minutes");
    expect(templateLabel("summary", "en")).toBe("Summary");
    expect(templateLabel("action_items", "en")).toBe("Action Items");
  });

  it("未知 id は「メモ」/ \"Notes\" へフォールバック", () => {
    expect(templateLabel("unknown", "ja")).toBe("メモ");
    expect(templateLabel("", "ja")).toBe("メモ");
    expect(templateLabel("unknown", "en")).toBe("Notes");
  });

  it("minutes は『議事録』（DetailView の UI ラベル『AI議事録』とは別物）", () => {
    expect(templateLabel("minutes", "ja")).toBe("議事録");
  });
});

describe("findSummary", () => {
  const summaries: Summary[] = [
    { template_id: "summary", content: "s", action_items: [] },
    { template_id: "minutes", content: "m", action_items: [] },
  ];

  it("一致するテンプレの要約を返す", () => {
    expect(findSummary(summaries, "minutes")?.content).toBe("m");
    expect(findSummary(summaries, "summary")?.content).toBe("s");
  });

  it("無ければ undefined", () => {
    expect(findSummary(summaries, "action_items")).toBeUndefined();
    expect(findSummary([], "minutes")).toBeUndefined();
  });
});
