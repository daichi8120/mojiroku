// ミーティングに質問（Studio 11・モック / ローカル RAG の先取り）。
// 実際の RAG はまだ無い。回答はキーワードベースの固定モック。PreviewTag で明示。
import { useEffect, useRef, useState } from "react";
import { Drawer } from "@/components/ui";
import { PreviewTag } from "@/components/composite";
import { SendIcon, SparklesIcon, XIcon } from "@/components/icons";

interface Msg {
  id: number;
  role: "user" | "ai";
  text: string;
  cites?: string[];
}

const SUGGESTIONS = ["3行でまとめて", "決定事項は？", "アクションは？"];

function mockAnswer(q: string): { text: string; cites: string[] } {
  if (/決定|決まっ/.test(q)) {
    return {
      text: "決定事項は2点です。配布フローの確定と、オンボーディングの刷新方針。初回画面で「ローカル完結・基本無料」を主役に据えます。",
      cites: ["01:02"],
    };
  }
  if (/アクション|宿題|ネクスト|やること|todo/i.test(q)) {
    return {
      text: "ネクストアクション:\n・初回画面のデザイン案を作成 — 佐藤\n・モデルDL進捗UIを実装 — 鈴木\n次回までに共有します。",
      cites: ["00:39", "01:20"],
    };
  }
  if (/3行|まとめ|要約/.test(q)) {
    return {
      text: "・未署名ビルドの初回起動手順に画像を追加\n・オンボーディングを「ローカル完結・基本無料」中心に刷新\n・モデルDLを初回フローへ統合する方針で合意",
      cites: ["00:14"],
    };
  }
  return {
    text: "この録音の文字起こしから、ご質問に近い箇所を引用してお答えします。「決定事項は？」「アクションは？」のように聞いてみてください。",
    cites: ["00:00"],
  };
}

export function AskDrawer({
  open,
  onClose,
  title,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
}) {
  const [messages, setMessages] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const seq = useRef(0);
  const scrollRef = useRef<HTMLDivElement>(null);

  // 新着メッセージで最下部へスクロール。
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTo({ top: el.scrollHeight });
  }, [messages]);

  const send = (raw: string) => {
    const text = raw.trim();
    if (!text) return;
    const a = mockAnswer(text);
    setMessages((m) => [
      ...m,
      { id: ++seq.current, role: "user", text },
      { id: ++seq.current, role: "ai", text: a.text, cites: a.cites },
    ]);
    setInput("");
  };

  return (
    <Drawer open={open} onClose={onClose} width={468}>
      {/* ヘッダ */}
      <div className="flex shrink-0 items-center justify-between border-b border-line px-5 py-3.5">
        <div className="flex min-w-0 items-center gap-2.5">
          <span className="flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-[8px] bg-brand text-white">
            <SparklesIcon size={15} />
          </span>
          <div className="min-w-0">
            <div className="text-[14px] font-bold text-ink">ミーティングに質問</div>
            <div className="truncate text-[11px] text-faint">{title}</div>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {/* ローカル / BYOK は表示のみ */}
          <div className="flex items-center gap-1 rounded-[8px] border border-border-2 bg-surface-2 p-1">
            <span className="rounded-md bg-brand px-2.5 py-1 text-[11px] font-semibold text-white">
              ローカル
            </span>
            <span className="px-2.5 py-1 text-[11px] text-muted">BYOK</span>
          </div>
          <button
            onClick={onClose}
            aria-label="閉じる"
            className="flex h-7 w-7 items-center justify-center rounded-btn text-sub transition-colors hover:bg-hover hover:text-ink"
          >
            <XIcon size={16} />
          </button>
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-2 border-b border-line px-5 py-2">
        <PreviewTag />
        <span className="text-[11px] text-muted">
          ローカル RAG は近日。回答はプレビュー（モック）です。
        </span>
      </div>

      {/* チャット */}
      <div
        ref={scrollRef}
        className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 py-4"
      >
        {messages.length === 0 && (
          <div className="mt-6 text-center text-[12.5px] leading-relaxed text-muted">
            この録音について質問できます。
            <br />
            下のサジェストから試してみてください。
          </div>
        )}
        {messages.map((m) =>
          m.role === "user" ? (
            <div
              key={m.id}
              className="max-w-[78%] self-end rounded-[14px_14px_4px_14px] bg-brand px-3.5 py-2.5 text-[13.5px] leading-relaxed text-white"
            >
              {m.text}
            </div>
          ) : (
            <div key={m.id} className="max-w-[86%] self-start">
              <div className="mb-1.5 flex items-center gap-1.5">
                <span className="flex h-5 w-5 items-center justify-center rounded-[6px] bg-[rgba(99,102,241,0.16)] text-brand-lighter">
                  <SparklesIcon size={11} />
                </span>
                <span className="text-[11px] text-faint">mojiroku · ローカル生成</span>
              </div>
              <div className="rounded-[4px_14px_14px_14px] border border-border-2 bg-surface-2 px-4 py-3 text-[13.5px] leading-relaxed text-speech">
                <p className="whitespace-pre-wrap">{m.text}</p>
                {m.cites && m.cites.length > 0 && (
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    {m.cites.map((c) => (
                      <span
                        key={c}
                        className="rounded-md bg-[rgba(34,211,238,0.12)] px-1.5 py-0.5 font-mono text-[11px] text-teal"
                      >
                        {c}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ),
        )}
      </div>

      {/* 入力 */}
      <div className="shrink-0 border-t border-line px-5 pb-4 pt-3">
        <div className="mb-2.5 flex flex-wrap gap-1.5">
          {SUGGESTIONS.map((s) => (
            <button
              key={s}
              onClick={() => send(s)}
              className="rounded-full border border-border-2 bg-surface-2 px-3 py-1.5 text-[11.5px] text-body transition-colors hover:bg-hover"
            >
              {s}
            </button>
          ))}
        </div>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            send(input);
          }}
          className="flex items-center gap-2.5 rounded-[12px] border border-border-3 bg-surface-2 px-3 py-2"
        >
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="この録音について質問する…"
            className="min-w-0 flex-1 bg-transparent text-[13.5px] text-ink outline-none placeholder:text-dim"
          />
          <span className="shrink-0 text-[11px] text-dim">引用つき</span>
          <button
            type="submit"
            aria-label="送信"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px] bg-brand text-white transition-[filter] hover:brightness-110"
          >
            <SendIcon size={14} />
          </button>
        </form>
      </div>
    </Drawer>
  );
}
