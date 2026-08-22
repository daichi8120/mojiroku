import { describe, expect, it } from "vitest";
import {
  obsidianMarkdown,
  summaryMarkdown,
  transcriptMarkdown,
} from "./share";
import type { RecordingDetail } from "./types";

function detail(): RecordingDetail {
  return {
    recording: {
      id: "r1",
      source_type: "file",
      title: "週次",
      duration_ms: 65_000,
      sample_rate: 16_000,
      created_at: "2026-06-27T01:02:03Z",
    },
    transcript: {
      language: "ja",
      segments: [
        { idx: 0, start_ms: 0, end_ms: 1_000, text: "おはよう", speaker_id: "S1" },
        { idx: 1, start_ms: 1_000, end_ms: 2_000, text: "やあ", speaker_id: "S2" },
        { idx: 2, start_ms: 2_000, end_ms: 3_000, text: "（無名）", speaker_id: null },
      ],
    },
    summaries: [{ template_id: "minutes", content: "  # 決定\n- A  ", action_items: [] }],
    speakers: [
      { id: "S1", label: "話者1", display_name: "田中" },
      { id: "S2", label: "話者2", display_name: null },
    ],
  };
}

describe("発言単位の話者訂正が書き出しに反映される（Issue #19）", () => {
  it("訂正した speaker_id の表示名で書き出される", () => {
    const d = detail();
    // 2 件目（idx=1）を S2 → S1 へ訂正した状態。
    d.transcript.segments[1] = { ...d.transcript.segments[1], speaker_id: "S1" };
    const md = transcriptMarkdown(d, "ja");
    // S1 の display_name は「田中」。訂正した行もそちらで出る。
    expect(md).toContain("**田中**: やあ");
    // 既定ラベルのままだった「話者2」は、もうどの行にも出ない。
    expect(md).not.toContain("話者2");
  });

  it("話者不明に戻すと話者の接頭辞が消える", () => {
    const d = detail();
    d.transcript.segments[0] = { ...d.transcript.segments[0], speaker_id: null };
    const md = transcriptMarkdown(d, "ja");
    expect(md).not.toContain("**田中**: おはよう");
    expect(md).toContain("おはよう");
  });
});

describe("summaryMarkdown", () => {
  it("content を trim する", () => {
    expect(
      summaryMarkdown({ template_id: "x", content: "  hi \n", action_items: [] }),
    ).toBe("hi");
  });
});

describe("transcriptMarkdown", () => {
  it("既定は話者つき・時刻なし（display_name→label→既定）", () => {
    expect(transcriptMarkdown(detail(), "ja")).toBe(
      "**田中**: おはよう\n**話者2**: やあ\n（無名）",
    );
  });

  it("話者なし・時刻つき", () => {
    expect(
      transcriptMarkdown(detail(), "ja", { withSpeakers: false, withTimestamps: true }),
    ).toBe("`00:00` おはよう\n`00:01` やあ\n`00:02` （無名）");
  });

  it("en: 話者表に無い id は Speaker N（表示名/label はそのまま）", () => {
    const d = detail();
    d.speakers = []; // 表なし → speakerLabelFromId の言語別既定ラベルに落ちる
    expect(transcriptMarkdown(d, "en")).toBe(
      "**Speaker 1**: おはよう\n**Speaker 2**: やあ\n（無名）",
    );
  });
});

describe("obsidianMarkdown", () => {
  it("要約見出しは templateLabel 由来（minutes → ## 議事録）", () => {
    const md = obsidianMarkdown(detail(), "ja");
    expect(md).toContain("## 議事録"); // C2 リファクタ後も出力ラベルは「議事録」のまま
    expect(md).toContain("# 週次");
    expect(md).toContain("## 文字起こし");
  });

  it("en: 見出しが英語になる（Minutes / Transcript）", () => {
    const md = obsidianMarkdown(detail(), "en");
    expect(md).toContain("## Minutes");
    expect(md).toContain("## Transcript");
    expect(md).not.toContain("## 文字起こし");
  });

  it("en: title 未設定の既定は Meeting", () => {
    const d = detail();
    d.recording.title = null;
    expect(obsidianMarkdown(d, "en")).toContain("# Meeting");
  });
});
