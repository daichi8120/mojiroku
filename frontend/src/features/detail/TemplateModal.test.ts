import { describe, expect, it } from "vitest";
import { shouldAutoStart } from "./TemplateModal";

// 生成を「開いただけで」始めてよいのはローカルで新規作成のときだけ（#107）。
// クラウドは文字起こしを外部へ送るので、必ず利用者が送信ボタンを押す。
describe("shouldAutoStart", () => {
  it("starts local generation of a new summary at once", () => {
    expect(shouldAutoStart("local", false)).toBe(true);
  });
  it("never starts cloud generation by itself", () => {
    expect(shouldAutoStart("cloud", false)).toBe(false);
    expect(shouldAutoStart("cloud", true)).toBe(false);
  });
  it("waits while the engine is unknown", () => {
    expect(shouldAutoStart(null, false)).toBe(false);
  });
  it("asks before replacing an existing summary", () => {
    expect(shouldAutoStart("local", true)).toBe(false);
  });
});
