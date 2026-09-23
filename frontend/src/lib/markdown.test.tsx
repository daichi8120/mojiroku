import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Markdown, parseMarkdown } from "./markdown";

const minutes = `# 議題
- リリース範囲の確認
  - 結合テスト
# 決定事項
1. 月曜にリリースする
2) 次回は木曜
# ToDo
- [ ] テストを完了する（**佐藤**、金曜）
- [x] 進捗バーを入れる

| 担当 | 期限 |
|---|---|
| 佐藤 | 金曜 |
---
補足の段落。
続きの行。`;

describe("parseMarkdown (#106)", () => {
  it("reads the structure the summary prompt asks for", () => {
    const kinds = parseMarkdown(minutes).map((b) => b.kind);
    expect(kinds).toEqual(["heading", "list", "heading", "list", "heading", "list", "table", "rule", "para"]);
    const first = parseMarkdown(minutes)[1];
    expect(first.kind === "list" && first.items.map((i) => i.depth)).toEqual([0, 1]);
    const todo = parseMarkdown(minutes)[5];
    expect(todo.kind === "list" && todo.items.map((i) => i.checked)).toEqual([false, true]);
  });

  it("renders without any Markdown markers left in the text", () => {
    const html = renderToStaticMarkup(<Markdown text={minutes} />);
    expect(html).not.toMatch(/(^|>)\s*#/);
    expect(html).not.toContain("**");
    expect(html).not.toContain("|---|");
    expect(html).toContain("<strong");
    expect(html).toContain("<h4");
    expect(html).toContain("<table");
  });

  it("never interprets HTML from the model", () => {
    const html = renderToStaticMarkup(<Markdown text={'<img src=x onerror="alert(1)"> **a**'} />);
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;img");
  });

  it("keeps a lone Japanese note asterisk as text", () => {
    const html = renderToStaticMarkup(<Markdown text={"※ 注意 * 仮の値"} />);
    expect(html).not.toContain("<em>");
  });
});
