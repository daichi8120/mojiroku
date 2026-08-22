import { describe, expect, it } from "vitest";
import { translateError } from "@/i18n";
import ja from "@/i18n/ja";
import en from "@/i18n/en";

describe("発言単位の話者訂正のエラーが翻訳される（Issue #19）", () => {
  // Rust の core_err が Display 接頭辞を外して返す文字列。
  const KEYS = ["error.speaker.unknown_for_recording", "error.segment.not_found"];

  it("ja: キーが辞書の文言に置き換わる", () => {
    for (const k of KEYS) {
      const out = translateError(k, ja);
      expect(out).not.toBe(k);
      expect(out).not.toContain("error.");
    }
  });

  it("en: 同上", () => {
    for (const k of KEYS) {
      const out = translateError(k, en);
      expect(out).not.toBe(k);
      expect(out).not.toContain("error.");
    }
  });

  it("接頭辞が付くと翻訳されない（core_err を通し忘れた場合の回帰）", () => {
    const withPrefix = "db error: error.segment.not_found";
    expect(translateError(withPrefix, ja)).toBe(withPrefix);
  });
});
