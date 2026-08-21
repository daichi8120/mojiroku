// translateError: Rust の Err（"error.<domain>.<cause>[: detail]"）→ アプリ言語の文言。
// 仕様: 既知キーは辞書文言 / 詳細は "文言 (詳細)" で連結 / 未知キーは原文フォールバック /
// 詳細が別のキーなら入れ子で翻訳（例: error.recording.mic_start: error.mic.busy）。
import { describe, expect, it } from "vitest";
import { translateError } from "./index";
import ja from "./ja";
import en from "./en";

describe("translateError", () => {
  it("既知キーはアプリ言語の文言に置き換える", () => {
    expect(translateError("error.mic.busy", ja)).toBe("すでに録音中です");
    expect(translateError("error.mic.busy", en)).toBe("Already recording");
  });

  it("詳細つきキーは「文言 (詳細)」に連結する", () => {
    expect(translateError("error.mic.input_config: BuildStreamError", ja)).toBe(
      `${ja.errors["error.mic.input_config"]} (BuildStreamError)`,
    );
    // 詳細の中に ": " が含まれても最初の区切りだけでキーを切り出す。
    expect(translateError("error.summarize.sidecar_failed: ggml: out of memory", en)).toBe(
      `${en.errors["error.summarize.sidecar_failed"]} (ggml: out of memory)`,
    );
  });

  it("未知キー・生文字列・Error オブジェクトは原文をそのまま返す", () => {
    expect(translateError("unexpected panic: oh no", ja)).toBe("unexpected panic: oh no");
    expect(translateError("error.unknown.key", ja)).toBe("error.unknown.key");
    expect(translateError(new Error("boom"), ja)).toBe("Error: boom");
  });

  it("詳細が別のキーの場合は入れ子で翻訳する", () => {
    expect(translateError("error.recording.mic_start: error.mic.busy", ja)).toBe(
      `${ja.errors["error.recording.mic_start"]} (${ja.errors["error.mic.busy"]})`,
    );
    // 入れ子の詳細（キー: 生詳細）も再帰的に処理される。
    expect(translateError("error.recording.mic_start: error.mic.input_config: cpal", en)).toBe(
      `${en.errors["error.recording.mic_start"]} (${en.errors["error.mic.input_config"]} (cpal))`,
    );
  });

  it("ja/en の errors 辞書はキーが一致する（Record 型のため tsc では検出できない）", () => {
    expect(Object.keys(en.errors).sort()).toEqual(Object.keys(ja.errors).sort());
  });

  it("errors 辞書のキーはすべて error.<domain>.<cause> 形式", () => {
    for (const key of Object.keys(ja.errors)) {
      expect(key).toMatch(/^error\.[a-z_]+\.[a-z_]+$/);
    }
  });
});
