// 生成 AI へ渡す経路のテスト。
//
// 守りたい性質はひとつ: **会議の内容が URL に載らないこと**。
// 2026-08-29 まで 1500 字以内のプロンプトを `?q=` で渡しており、文字起こしが
// ブラウザの履歴・同期・拡張機能に残っていた。長さで挙動が変わる形だったので、
// 「短い入力」でも載らないことをテストで固定する。
import { beforeEach, describe, expect, it, vi } from "vitest";

const opened: string[] = [];
const copied: string[] = [];

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: async (url: string) => {
    opened.push(url);
  },
}));

// copyText は navigator.clipboard を使う（jsdom には無いので生やす）
Object.defineProperty(globalThis, "navigator", {
  value: { clipboard: { writeText: async (t: string) => void copied.push(t) } },
  configurable: true,
  writable: true,
});

const { openInAi, AI_CHAT_URL } = await import("./share");

const SECRET = "社外秘の発言そのもの";

describe("openInAi", () => {
  beforeEach(() => {
    opened.length = 0;
    copied.length = 0;
  });

  it.each(["chatgpt", "claude"] as const)("%s: 開く URL にクエリを付けない", async (p) => {
    await openInAi(p, `短いプロンプト ${SECRET}`);
    expect(opened).toEqual([AI_CHAT_URL[p]]);
    expect(opened[0]).not.toContain("?");
  });

  it("短いプロンプトでも本文を URL に載せない", async () => {
    await openInAi("chatgpt", `${SECRET}`); // 1500 字より遥かに短い
    expect(opened[0]).not.toContain(SECRET);
    expect(opened[0]).not.toContain(encodeURIComponent(SECRET));
  });

  it("長いプロンプトでも同じ URL を開く（長さで挙動が変わらない）", async () => {
    await openInAi("claude", "あ".repeat(5000));
    expect(opened).toEqual([AI_CHAT_URL.claude]);
  });

  it("プロンプトは必ずクリップボードへコピーする", async () => {
    await openInAi("chatgpt", SECRET);
    expect(copied).toEqual([SECRET]);
  });
});
