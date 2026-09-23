import { describe, expect, it } from "vitest";
import {
  SPEAKER_PALETTE,
  elapsedSeconds,
  formatDateTime,
  formatDuration,
  recordingState,
  recordingTitle,
  segmentAt,
  formatDurationHuman,
  formatEventTime,
  formatTimestamp,
  speakerIndex,
  speakerLabelFromId,
  speakerName,
  type Speaker,
} from "./types";

describe("formatTimestamp", () => {
  it("ms → ゼロ詰め mm:ss", () => {
    expect(formatTimestamp(0)).toBe("00:00");
    expect(formatTimestamp(65_000)).toBe("01:05");
    expect(formatTimestamp(599_000)).toBe("09:59");
    // mm:ss は時間繰り上げしない（60 分超でも分が伸びる）。
    expect(formatTimestamp(3_600_000)).toBe("60:00");
  });
});

describe("elapsedSeconds", () => {
  const t0 = 1_700_000_000_000;

  it("開始からの差分を秒に切り捨てる", () => {
    expect(elapsedSeconds(t0, t0)).toBe(0);
    expect(elapsedSeconds(t0, t0 + 999)).toBe(0);
    expect(elapsedSeconds(t0, t0 + 1_000)).toBe(1);
    expect(elapsedSeconds(t0, t0 + 1_999)).toBe(1);
    expect(elapsedSeconds(t0, t0 + 3_600_000)).toBe(3600);
  });

  it("startedAt が null なら 0", () => {
    expect(elapsedSeconds(null, t0 + 60_000)).toBe(0);
  });

  it("システム時刻が巻き戻っても負にならない", () => {
    expect(elapsedSeconds(t0, t0 - 5_000)).toBe(0);
  });

  it("tick が間引かれても値が壊れない（累積方式との違い）", () => {
    // setInterval が 30 分ぶん間引かれても、壁時計差分なら正しい値が出る。
    // 累積方式（elapsed + 1）だと、この 1800 秒が丸ごと失われる。
    const thirtyMin = 30 * 60_000;
    expect(elapsedSeconds(t0, t0 + thirtyMin)).toBe(1800);
  });
});

describe("formatDuration", () => {
  it("1 時間未満は m:ss", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(65_000)).toBe("1:05");
  });
  it("1 時間以上は h:mm:ss", () => {
    expect(formatDuration(3_661_000)).toBe("1:01:01");
  });
});

describe("formatDurationHuman", () => {
  it("ja: 分のみ / 時間＋分 / ちょうど時間", () => {
    expect(formatDurationHuman(24 * 60_000, "ja")).toBe("24分");
    expect(formatDurationHuman(72 * 60_000, "ja")).toBe("1時間12分");
    expect(formatDurationHuman(120 * 60_000, "ja")).toBe("2時間");
  });
  it("en: min のみ / hr + min / ちょうど hr", () => {
    expect(formatDurationHuman(24 * 60_000, "en")).toBe("24 min");
    expect(formatDurationHuman(72 * 60_000, "en")).toBe("1 hr 12 min");
    expect(formatDurationHuman(120 * 60_000, "en")).toBe("2 hr");
  });
});

describe("formatEventTime", () => {
  it("今日の予定は言語別の接頭辞（今日/Today）", () => {
    const now = new Date();
    const pad = (n: number) => n.toString().padStart(2, "0");
    // ローカル壁時計表記（オフセットなし）で「今日の 23:59」を作る
    const start = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T23:59:00`;
    expect(formatEventTime(start, "ja")).toBe("今日 23:59");
    expect(formatEventTime(start, "en")).toBe("Today 23:59");
  });
});

describe("speaker helpers", () => {
  it("speakerIndex: S 番号は剰余で巡回し、必ず範囲内", () => {
    expect(speakerIndex("S1")).toBe(0);
    expect(speakerIndex("S2")).toBe(1);
    const len = SPEAKER_PALETTE.length;
    expect(speakerIndex(`S${len + 1}`)).toBe(0); // 巡回して先頭へ
    const anon = speakerIndex("anon");
    expect(anon).toBeGreaterThanOrEqual(0);
    expect(anon).toBeLessThan(len);
  });

  it("speakerLabelFromId: S番号→話者N / Speaker N、その他はそのまま", () => {
    expect(speakerLabelFromId("S1", "ja")).toBe("話者1");
    expect(speakerLabelFromId("S12", "ja")).toBe("話者12");
    expect(speakerLabelFromId("S1", "en")).toBe("Speaker 1");
    expect(speakerLabelFromId("foo", "ja")).toBe("foo");
    expect(speakerLabelFromId("foo", "en")).toBe("foo");
  });

  it("speakerName: 表示名 → ラベル → 既定ラベル の順でフォールバック", () => {
    const speakers: Speaker[] = [
      { id: "S1", label: "話者1", display_name: "田中" },
      { id: "S2", label: "話者2", display_name: null },
    ];
    expect(speakerName("S1", speakers, "ja")).toBe("田中"); // display_name
    expect(speakerName("S2", speakers, "ja")).toBe("話者2"); // display_name null → label
    expect(speakerName("S3", speakers, "ja")).toBe("話者3"); // 表に無い → 既定ラベル
    expect(speakerName("S1", undefined, "ja")).toBe("話者1"); // speakers 無し
    expect(speakerName("S3", speakers, "en")).toBe("Speaker 3"); // en の既定ラベル
  });
});

describe("recordingTitle / formatDateTime (#105)", () => {
  const base = { id: "r", sample_rate: 16000, duration_ms: 1000, created_at: "2026-09-21T01:00:00Z" };
  it("keeps a real title", () => {
    expect(recordingTitle({ ...base, source_type: "mic", title: " 定例 " }, "ja")).toBe("定例");
  });
  it("names untitled recordings by kind and time, the same way everywhere", () => {
    const ja = recordingTitle({ ...base, source_type: "mic", title: null }, "ja");
    expect(ja.startsWith("マイク録音（")).toBe(true);
    expect(ja).not.toContain("無題");
    const en = recordingTitle({ ...base, source_type: "live", title: "  " }, "en");
    expect(en.startsWith("Meeting (")).toBe(true);
  });
  it("treats the backend's default titles as untitled", () => {
    for (const title of ["録音", "Recording", "会議", "Meeting"]) {
      expect(recordingTitle({ ...base, source_type: "mic", title }, "ja").startsWith("マイク録音（")).toBe(true);
    }
  });
    it("shows dates without seconds", () => {
    const s = formatDateTime("2026-09-23T02:03:45Z", "ja");
    expect(s).not.toMatch(/:\d\d:\d\d/);
  });
});

describe("segmentAt (#110)", () => {
  const seg = (idx: number, start_ms: number) => ({ idx, start_ms, end_ms: start_ms + 900, text: "", speaker_id: null });
  const segs = [seg(0, 0), seg(1, 1000), seg(2, 2500), seg(3, 4000)];
  it("finds the segment under the playhead", () => {
    expect(segmentAt(segs, 0)).toBe(0);
    expect(segmentAt(segs, 999)).toBe(0);
    expect(segmentAt(segs, 1000)).toBe(1);
    expect(segmentAt(segs, 3000)).toBe(2);
    expect(segmentAt(segs, 99999)).toBe(3);
  });
  it("is null before the first segment or with no segments", () => {
    expect(segmentAt([seg(0, 500)], 100)).toBeNull();
    expect(segmentAt([], 100)).toBeNull();
  });
});

describe("recordingState (#109)", () => {
  const recording = { id: "r", source_type: "mic" as const, title: null, duration_ms: 1, sample_rate: 16000, created_at: "" };
  const row = (over: Partial<import("./types").RecordingRow>) => ({
    recording, segment_count: 10, speaker_count: 0, summary_count: 0, latest_job: null, ...over,
  });
  const job = (status: string) => ({ kind: "transcribe", status, error: null });
  it("puts processing and failure first", () => {
    expect(recordingState(row({ latest_job: job("running") }))).toBe("processing");
    expect(recordingState(row({ latest_job: job("pending"), segment_count: 0 }))).toBe("processing");
    expect(recordingState(row({ latest_job: job("failed"), summary_count: 2 }))).toBe("failed");
  });
  it("then transcript and summary", () => {
    expect(recordingState(row({ segment_count: 0 }))).toBe("untranscribed");
    expect(recordingState(row({ latest_job: job("done"), summary_count: 1 }))).toBe("summarized");
    expect(recordingState(row({ latest_job: job("canceled") }))).toBe("transcribed");
  });
});
