// 要約・議事録の Markdown を React 要素に描く小さなレンダラー（#106）。
//
// 要約プロンプトは「# 議題 / # 決定事項 …」の見出しと箇条書きで出力させている。以前は本文をそのまま
// 出していたので `#` や `- ` が記号のまま見えていた。
//
// ⚠️ HTML は一切解釈しない（dangerouslySetInnerHTML を使わない）。モデルの出力は信頼できない入力で、
// 文字列はすべて React のテキストノードとして描く。リンクもテキストとして出し、遷移させない。
// 対応する記法は LLM が実際に出すものに絞る: 見出し・箇条書き（入れ子）・番号付き・チェックボックス・
// 区切り線・表・太字・斜体・インラインコード。コピー/書き出しは元の Markdown のまま使う。
import { Fragment, type ReactNode } from "react";
import { cx } from "./cx";

type Block =
  | { kind: "heading"; level: number; text: string }
  | { kind: "list"; ordered: boolean; items: ListItem[] }
  | { kind: "rule" }
  | { kind: "table"; rows: string[][] }
  | { kind: "para"; text: string };

interface ListItem {
  depth: number;
  text: string;
  checked: boolean | null;
}

const BULLET = /^(\s*)(?:[-*+•・])\s+(.*)$/;
const ORDERED = /^(\s*)\d+[.)]\s+(.*)$/;
const HEADING = /^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
const RULE = /^\s{0,3}(?:-{3,}|\*{3,}|_{3,})\s*$/;
const TABLE_ROW = /^\s*\|.*\|\s*$/;
const TABLE_SEP = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;
const CHECK = /^\[( |x|X)\]\s+(.*)$/;

export function parseMarkdown(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let para: string[] = [];
  const flush = () => {
    if (para.length) blocks.push({ kind: "para", text: para.join("\n") });
    para = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!line.trim()) {
      flush();
      continue;
    }
    const h = HEADING.exec(line);
    if (h) {
      flush();
      blocks.push({ kind: "heading", level: h[1].length, text: h[2] });
      continue;
    }
    if (RULE.test(line)) {
      flush();
      blocks.push({ kind: "rule" });
      continue;
    }
    if (TABLE_ROW.test(line)) {
      flush();
      const rows: string[][] = [];
      while (i < lines.length && TABLE_ROW.test(lines[i])) {
        if (!TABLE_SEP.test(lines[i])) {
          rows.push(
            lines[i]
              .trim()
              .replace(/^\||\|$/g, "")
              .split("|")
              .map((c) => c.trim()),
          );
        }
        i++;
      }
      i--;
      blocks.push({ kind: "table", rows });
      continue;
    }
    const b = BULLET.exec(line);
    const o = b ? null : ORDERED.exec(line);
    if (b || o) {
      flush();
      const ordered = !!o;
      const m = (b ?? o)!;
      const item = toItem(m[1], m[2]);
      const last = blocks[blocks.length - 1];
      if (last && last.kind === "list" && last.ordered === ordered) last.items.push(item);
      else blocks.push({ kind: "list", ordered, items: [item] });
      continue;
    }
    // 箇条書き直後のインデントされた行は、その項目の続き。
    const last = blocks[blocks.length - 1];
    if (!para.length && last && last.kind === "list" && /^\s{2,}\S/.test(line)) {
      last.items[last.items.length - 1].text += "\n" + line.trim();
      continue;
    }
    para.push(line.trim());
  }
  flush();
  return blocks;
}

function toItem(indent: string, rest: string): ListItem {
  const depth = Math.min(3, Math.floor(indent.replace(/\t/g, "  ").length / 2));
  const c = CHECK.exec(rest);
  return c
    ? { depth, text: c[2], checked: c[1].toLowerCase() === "x" }
    : { depth, text: rest, checked: null };
}

// インライン: **太字** / __太字__ / *斜体* / `code` / [text](url)。
// 日本語の「*」単独（注記）を斜体と誤読しないよう、斜体は前後が空白でない * のペアに限る。
const INLINE = /(\*\*[^*\n]+?\*\*|__[^_\n]+?__|`[^`\n]+?`|\[[^\]\n]+?\]\([^)\s]+?\)|\*[^*\s][^*\n]*?[^*\s]\*|\*[^*\s]\*)/g;

export function renderInline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  let key = 0;
  for (const m of text.matchAll(INLINE)) {
    const idx = m.index ?? 0;
    if (idx > last) out.push(text.slice(last, idx));
    const tok = m[0];
    if (tok.startsWith("**") || tok.startsWith("__")) {
      out.push(
        <strong key={key++} className="font-semibold text-ink">
          {tok.slice(2, -2)}
        </strong>,
      );
    } else if (tok.startsWith("`")) {
      out.push(
        <code key={key++} className="rounded-tag bg-hover px-1 py-px font-mono text-[13px]">
          {tok.slice(1, -1)}
        </code>,
      );
    } else if (tok.startsWith("[")) {
      const lm = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(tok)!;
      out.push(
        <span key={key++} className="text-brand-light" title={lm[2]}>
          {lm[1]}
        </span>,
      );
    } else {
      out.push(<em key={key++}>{tok.slice(1, -1)}</em>);
    }
    last = idx + tok.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

function withBreaks(text: string): ReactNode[] {
  return text.split("\n").map((part, i) => (
    <Fragment key={i}>
      {i > 0 && <br />}
      {renderInline(part)}
    </Fragment>
  ));
}

/** 要約・議事録の本文。見出し・箇条書き・表などを描く。 */
export function Markdown({ text, className }: { text: string; className?: string }) {
  const blocks = parseMarkdown(text);
  return (
    <div className={cx("flex flex-col gap-2.5 text-[15px] leading-[1.75] text-body", className)}>
      {blocks.map((b, i) => {
        switch (b.kind) {
          case "heading":
            return b.level <= 2 ? (
              <h4
                key={i}
                className={cx("text-[15px] font-bold text-ink", i > 0 && "mt-2 border-t border-line pt-3")}
              >
                {renderInline(b.text)}
              </h4>
            ) : (
              <h5 key={i} className="mt-1 text-[14px] font-semibold text-ink">
                {renderInline(b.text)}
              </h5>
            );
          case "rule":
            return <hr key={i} className="border-line" />;
          case "table":
            return (
              <div key={i} className="overflow-x-auto">
                <table className="w-full border-collapse text-[14px]">
                  <tbody>
                    {b.rows.map((r, ri) => (
                      <tr key={ri} className="border-b border-line">
                        {r.map((c, ci) => {
                          const Cell = ri === 0 ? "th" : "td";
                          return (
                            <Cell
                              key={ci}
                              className={cx(
                                "px-2 py-1.5 text-left align-top",
                                ri === 0 && "font-semibold text-ink",
                              )}
                            >
                              {renderInline(c)}
                            </Cell>
                          );
                        })}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          case "list": {
            const Tag = b.ordered ? "ol" : "ul";
            return (
              <Tag key={i} className="flex flex-col gap-1">
                {b.items.map((it, j) => (
                  <li
                    key={j}
                    className="flex gap-2"
                    style={{ paddingLeft: it.depth * 18 }}
                  >
                    <span aria-hidden className="w-4 shrink-0 text-center text-dim">
                      {it.checked !== null ? (it.checked ? "☑" : "☐") : b.ordered ? `${j + 1}.` : "•"}
                    </span>
                    <span className={cx("min-w-0", it.checked && "text-muted line-through")}>
                      {withBreaks(it.text)}
                    </span>
                  </li>
                ))}
              </Tag>
            );
          }
          default:
            return <p key={i}>{withBreaks(b.text)}</p>;
        }
      })}
    </div>
  );
}
