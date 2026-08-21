// 横断ダイジェスト（Studio 16）— モック。定例シリーズを横断して Mac 内で集計。
// ⚠️ バックエンド未実装。@/lib/mockData の DIGEST を使い PreviewTag を表示する。
import { useApp } from "@/lib/app";
import { cx } from "@/lib/cx";
import { speakerInk } from "@/lib/types";
import { DIGEST } from "@/lib/mockData";
import { Badge, SectionLabel } from "@/components/ui";
import { PreviewTag, PrivacyBar } from "@/components/composite";
import { CheckIcon, ClockIcon, LayersIcon } from "@/components/icons";

// assignee は名前ごとに一貫した話者色（SPEAKER 配色と整合: 田中=indigo, 佐藤=teal, 鈴木=amber）。
const ASSIGNEE_SPEAKER: Record<string, string> = {
  田中: "S1",
  佐藤: "S2",
  鈴木: "S3",
};
const assigneeColor = (name: string) => speakerInk(ASSIGNEE_SPEAKER[name] ?? name).text;

export function DigestView() {
  const { navigate } = useApp();
  const lastIdx = DIGEST.sessions.length - 1;

  return (
    <div className="mx-auto flex max-w-[760px] flex-col gap-7 px-8 py-10">
      {/* ヘッダ */}
      <header className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 items-start gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[9px] bg-brand/15 text-brand-light">
            <LayersIcon size={18} />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="text-[17px] font-bold text-ink">{DIGEST.series}</h1>
              <Badge tone="indigo">シリーズ</Badge>
              <PreviewTag />
            </div>
            <p className="mt-1 text-[13px] text-muted">
              履歴を横断して Mac 内で集計
            </p>
          </div>
        </div>
        <button
          onClick={() => navigate({ view: "history" })}
          className="shrink-0 text-[11.5px] text-sub transition-colors hover:text-ink"
        >
          履歴へ
        </button>
      </header>

      {/* 過去のセッション（タイムライン） */}
      <section>
        <div className="flex items-center justify-between">
          <SectionLabel>過去のセッション</SectionLabel>
          <span className="text-[11.5px] text-muted">
            過去{DIGEST.sessions.length}回 · 9月
          </span>
        </div>
        <div className="mt-4 flex items-center">
          {DIGEST.sessions.map((s, i) => {
            const active = i === lastIdx;
            return (
              <div key={s.date} className="flex flex-1 items-center">
                <div className="flex flex-1 flex-col items-center gap-1.5">
                  <span
                    className={cx(
                      "rounded-full",
                      active
                        ? "h-3 w-3 bg-brand shadow-[0_0_0_4px_rgba(99,102,241,0.18)]"
                        : "h-[11px] w-[11px] bg-border-2",
                    )}
                  />
                  <span
                    className={cx(
                      "font-mono text-[10px]",
                      active ? "text-brand-light" : "text-faint",
                    )}
                  >
                    {s.date}
                  </span>
                  <span
                    className={cx(
                      "text-center text-[11px] leading-tight",
                      active ? "text-body" : "text-muted",
                    )}
                  >
                    {s.title}
                  </span>
                </div>
                {i < lastIdx && (
                  <span
                    className="h-[1.5px] flex-1 self-start"
                    style={{
                      marginTop: 5,
                      background:
                        i === lastIdx - 1
                          ? "linear-gradient(90deg,#2A3140,#6366F1)"
                          : "#2A3140",
                    }}
                  />
                )}
              </div>
            );
          })}
        </div>
      </section>

      {/* 今月の決定事項 */}
      <section className="rounded-card border border-border bg-surface-2 px-[18px] py-4">
        <div className="mb-3 text-[12px] font-bold tracking-[0.03em] text-brand-light">
          今月の決定事項
        </div>
        <div className="flex flex-col gap-2.5">
          {DIGEST.decisions.map((d) => (
            <div key={d} className="flex gap-2.5">
              <span className="mt-0.5 shrink-0 text-green">
                <CheckIcon size={15} />
              </span>
              <span className="text-[13px] leading-[1.6] text-body">{d}</span>
            </div>
          ))}
        </div>
      </section>

      {/* 未完了のアクション（横断） */}
      <section className="rounded-card border border-border bg-surface px-[18px] py-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="flex items-center gap-2 text-[12px] font-bold tracking-[0.03em] text-sub">
            <ClockIcon size={14} className="text-muted" />
            未完了のアクション（横断）
          </span>
          <Badge tone="red">{DIGEST.openActions.length}件</Badge>
        </div>
        <div className="flex flex-col">
          {DIGEST.openActions.map((a, i) => (
            <div
              key={a.text}
              className={cx(
                "flex items-center gap-3 py-2",
                i < DIGEST.openActions.length - 1 && "border-b border-line",
              )}
            >
              <span className="h-4 w-4 shrink-0 rounded-[5px] border-2 border-border-2" />
              <span className="flex-1 text-[13px] text-body">{a.text}</span>
              <span
                className="shrink-0 text-[12px] font-medium"
                style={{ color: assigneeColor(a.assignee) }}
              >
                {a.assignee}
              </span>
              <span
                className={cx(
                  "shrink-0 font-mono text-[10.5px]",
                  a.overdue ? "text-red-light" : "text-muted",
                )}
              >
                {a.age}
              </span>
            </div>
          ))}
        </div>
      </section>

      {/* 繰り返し出る話題 */}
      <section className="flex flex-wrap items-center gap-2">
        <span className="text-[11.5px] text-muted">繰り返し出る話題</span>
        {DIGEST.recurring.map((r) => (
          <span
            key={r.topic}
            className="rounded-full border border-border bg-surface-2 px-3 py-1 text-[11.5px] text-body"
          >
            {r.topic} <span className="font-mono font-medium text-brand-light">×{r.count}</span>
          </span>
        ))}
      </section>

      <PrivacyBar>履歴を横断してこの Mac の中で集計。クラウドへ送信しません。</PrivacyBar>
    </div>
  );
}
