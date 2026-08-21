// 「AIに送る / コピー / 書き出し」ポップオーバー（Studio 10・フロント完結の実機能）。
// すべてユーザー操作起点のコピー / ブラウザ起動。自動送信なし。
import { useRef, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  aiPrompt,
  copyText,
  exportBaseName,
  obsidianMarkdown,
  openInAi,
  summaryMarkdown,
  transcriptMarkdown,
  transcriptPlain,
  transcriptSrt,
  type AiProvider,
} from "@/lib/share";
import { exportTextFile, exportToNotion, exportToSlack } from "@/lib/tauri";
import { printMeetingPdf } from "@/lib/print";
import { findSummary } from "@/lib/templates";
import { useApp, type ToastKind } from "@/lib/app";
import { translateError, useI18n } from "@/i18n";
import type { RecordingDetail } from "@/lib/types";
import { Popover, DropdownCaret } from "@/components/ui";
import {
  ClockIcon,
  CopyIcon,
  MessageIcon,
  SendIcon,
  ShieldIcon,
  UsersIcon,
} from "@/components/icons";

function Sec({ children }: { children: ReactNode }) {
  return (
    <div className="px-2 pb-1 pt-2 text-[10.5px] font-bold uppercase tracking-[0.07em] text-dim">
      {children}
    </div>
  );
}

// 書き出し（.md/.txt/.srt/PDF）の枠付きファイルボタン。同一クラスの反復を 1 箇所に集約。
function ExportFileButton({
  onClick,
  children,
}: {
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className="rounded-[7px] border border-border-3 bg-popover-2 px-2.5 py-1 text-[11px] text-body transition-colors hover:bg-hover"
    >
      {children}
    </button>
  );
}

function CopyRow({
  icon,
  title,
  sub,
  accent,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  sub?: string;
  accent?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="flex w-full items-center gap-2.5 rounded-[9px] px-2 py-2 text-left transition-colors hover:bg-popover-2"
    >
      <span
        className={
          accent
            ? "flex h-7 w-7 shrink-0 items-center justify-center rounded-[7px] bg-[rgba(99,102,241,0.15)] text-brand-lighter"
            : "flex h-7 w-7 shrink-0 items-center justify-center rounded-[7px] bg-surface-2 text-muted"
        }
      >
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13px] font-semibold text-ink">{title}</span>
        {sub && <span className="block text-[10.5px] text-faint">{sub}</span>}
      </span>
      <CopyIcon size={13} className="shrink-0 text-faint" />
    </button>
  );
}

/** 「連携」セクションの外部送信ボタン（Notion / Slack で DOM 同一。CopyRow とは別スタイル）。 */
function LinkRow({
  icon,
  title,
  sub,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  sub: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="mx-2 mb-1 flex w-[calc(100%-16px)] items-center gap-2.5 rounded-[9px] border border-border-3 bg-popover-2 px-2 py-2 text-left transition-colors hover:bg-hover"
    >
      <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-[7px] bg-surface-2 text-muted">
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13px] font-semibold text-ink">{title}</span>
        <span className="block text-[10.5px] text-faint">{sub}</span>
      </span>
    </button>
  );
}

export function SharePopover({ detail }: { detail: RecordingDetail }) {
  const { toast } = useApp();
  const { t, lang } = useI18n();
  // Notion 送信中フラグ。ポップオーバーは close で unmount されるが、SharePopover 自体は
  // マウントされ続けるため、送信の await 中に再オープン→再クリックで二重作成され得る。それを防ぐ。
  const notionSending = useRef(false);
  // Slack 送信中フラグ（同上。二重投稿を防ぐ）。
  const slackSending = useRef(false);

  const doCopy = async (
    close: () => void,
    text: string,
    msg: string = t.detail.share.copied,
    kind: ToastKind = "success",
  ) => {
    try {
      await copyText(text);
      toast(msg, kind);
    } catch (e) {
      toast(t.detail.share.copyFailed(translateError(e, t)), "error");
    } finally {
      close();
    }
  };

  // 保存ダイアログ（Rust 側で開く）→ 実ファイル書き出し。キャンセル時は何もしない。
  const doExport = async (
    close: () => void,
    content: string,
    ext: string,
    filterName: string,
  ) => {
    try {
      const saved = await exportTextFile(
        `${exportBaseName(detail)}.${ext}`,
        ext,
        filterName,
        content,
      );
      if (saved) toast(t.detail.share.exported, "success");
    } catch (e) {
      toast(t.detail.share.exportFailed(translateError(e, t)), "error");
    } finally {
      close();
    }
  };

  // Notion へ送信（要約 + 文字起こしをページ化）。ポップオーバーは即閉じて二重送信を防ぐ。
  // 未設定（トークン/親ページ）の場合はコマンドが設定誘導メッセージを返すので toast で案内。
  const doNotion = async (close: () => void) => {
    if (notionSending.current) return; // 送信中の再入を無視（二重ページ作成を防ぐ）
    notionSending.current = true;
    close();
    toast(t.detail.share.notionSending, "info");
    try {
      const url = await exportToNotion(detail.recording.id);
      toast(t.detail.share.notionSent, "success");
      if (url) void openUrl(url).catch(() => {});
    } catch (e) {
      toast(t.detail.share.notionFailed(translateError(e, t)), "error");
    } finally {
      notionSending.current = false;
    }
  };

  // Slack へ投稿（要約のみ。文字起こしは送らない）。要約が無ければコマンドが誘導メッセージを返す。
  const doSlack = async (close: () => void) => {
    if (slackSending.current) return; // 送信中の再入を無視（二重投稿を防ぐ）
    slackSending.current = true;
    close();
    toast(t.detail.share.slackSending, "info");
    try {
      await exportToSlack(detail.recording.id);
      toast(t.detail.share.slackSent, "success");
    } catch (e) {
      toast(t.detail.share.slackFailed(translateError(e, t)), "error");
    } finally {
      slackSending.current = false;
    }
  };

  const openAi = async (close: () => void, provider: AiProvider) => {
    try {
      await openInAi(provider, aiPrompt(detail, lang));
      toast(t.detail.share.aiOpened, "success");
    } catch (e) {
      toast(t.detail.share.aiOpenFailed(translateError(e, t)), "error");
    } finally {
      close();
    }
  };

  // 「議事録（Markdown）」は minutes テンプレの結果のみ。
  // summaries[0] は生成順依存で要約/アクション等になり得るためラベルと中身がずれる。
  const minutesRow = findSummary(detail.summaries, "minutes");

  // 「要約（3行）」は要約テンプレの結果。無ければ作成を促す。
  const summaryRow = findSummary(detail.summaries, "summary");

  return (
    <Popover
      width={312}
      trigger={({ open, toggle }) => (
        <button
          onClick={toggle}
          aria-haspopup="menu"
          aria-expanded={open}
          className="inline-flex h-7 items-center gap-1.5 rounded-[7px] px-2.5 text-[11.5px] font-semibold text-white transition-[filter] hover:brightness-110"
          style={{ background: "linear-gradient(180deg,#6366F1,#4F46E5)" }}
        >
          <SendIcon size={13} />
          {t.detail.share.trigger}
          <DropdownCaret />
        </button>
      )}
    >
      {(close) => (
        <div>
          <Sec>{t.detail.share.secCopy}</Sec>
          <CopyRow
            accent
            icon={<CopyIcon size={15} />}
            title={t.detail.share.minutesMd}
            sub={t.detail.share.minutesMdSub}
            onClick={() => {
              if (minutesRow) doCopy(close, summaryMarkdown(minutesRow));
              else {
                toast(t.detail.share.needMinutes, "info");
                close();
              }
            }}
          />
          <CopyRow
            icon={<MessageIcon size={15} />}
            title={t.detail.share.summaryRow}
            onClick={() => {
              if (summaryRow) {
                doCopy(close, summaryMarkdown(summaryRow));
              } else {
                toast(t.detail.share.needSummary, "info");
                close();
              }
            }}
          />
          <CopyRow
            icon={<UsersIcon size={15} />}
            title={t.detail.share.transcriptSpeakers}
            onClick={() => doCopy(close, transcriptMarkdown(detail, lang, { withSpeakers: true }))}
          />
          <CopyRow
            icon={<ClockIcon size={15} />}
            title={t.detail.share.transcriptTimestamps}
            onClick={() =>
              doCopy(
                close,
                transcriptMarkdown(detail, lang, { withSpeakers: true, withTimestamps: true }),
              )
            }
          />

          <div className="my-1.5 h-px bg-border-2" />
          <Sec>{t.detail.share.secAi}</Sec>
          <div className="flex gap-2 px-2 pb-1.5">
            <button
              onClick={() => openAi(close, "chatgpt")}
              className="h-[34px] flex-1 rounded-[9px] border border-border-3 bg-popover-2 text-[12px] font-semibold text-ink transition-colors hover:bg-hover"
            >
              {t.detail.share.openChatGpt}
            </button>
            <button
              onClick={() => openAi(close, "claude")}
              className="h-[34px] flex-1 rounded-[9px] border border-border-3 bg-popover-2 text-[12px] font-semibold text-ink transition-colors hover:bg-hover"
            >
              {t.detail.share.openClaude}
            </button>
          </div>
          <button
            onClick={() => doCopy(close, aiPrompt(detail, lang))}
            className="mx-2 mb-1.5 block h-[34px] w-[calc(100%-16px)] rounded-[9px] bg-[rgba(99,102,241,0.16)] text-[12px] font-semibold text-brand-lighter transition-[filter] hover:brightness-110"
          >
            {t.detail.share.copyWithPrompt}
          </button>

          <div className="my-1.5 h-px bg-border-2" />
          <Sec>{t.detail.share.secExport}</Sec>
          <div className="flex flex-wrap gap-1.5 px-2 pb-1.5">
            <button
              onClick={() =>
                doExport(close, obsidianMarkdown(detail, lang), "md", t.detail.share.fmtObsidian)
              }
              className="rounded-[7px] border border-brand/30 bg-[rgba(99,102,241,0.12)] px-2.5 py-1 text-[11px] font-semibold text-brand-lighter transition-colors hover:brightness-110"
            >
              {t.detail.share.obsidianNote}
            </button>
            <ExportFileButton
              onClick={() =>
                doExport(
                  close,
                  transcriptMarkdown(detail, lang, { withSpeakers: true }),
                  "md",
                  t.detail.share.fmtMarkdown,
                )
              }
            >
              .md
            </ExportFileButton>
            <ExportFileButton
              onClick={() =>
                doExport(close, transcriptPlain(detail, lang), "txt", t.detail.share.fmtText)
              }
            >
              .txt
            </ExportFileButton>
            <ExportFileButton
              onClick={() =>
                doExport(close, transcriptSrt(detail, lang), "srt", t.detail.share.fmtSrt)
              }
            >
              {t.detail.share.srtButton}
            </ExportFileButton>
            <ExportFileButton
              onClick={() => {
                try {
                  printMeetingPdf(detail, lang);
                } catch (e) {
                  toast(t.detail.share.pdfFailed(translateError(e, t)), "error");
                }
                close();
              }}
            >
              {t.detail.share.pdfButton}
            </ExportFileButton>
          </div>
          <p className="px-2 pb-2 text-[10px] text-faint">{t.detail.share.exportNote}</p>

          <div className="my-1.5 h-px bg-border-2" />
          <Sec>{t.detail.share.secIntegrations}</Sec>
          <LinkRow
            icon={<SendIcon size={15} />}
            title={t.detail.share.notionTitle}
            sub={t.detail.share.notionSub}
            onClick={() => doNotion(close)}
          />
          <LinkRow
            icon={<MessageIcon size={15} />}
            title={t.detail.share.slackTitle}
            sub={t.detail.share.slackSub}
            onClick={() => doSlack(close)}
          />
          <p className="px-2 pb-2 text-[10px] text-faint">{t.detail.share.integrationsNote}</p>

          <div className="mt-1 flex items-center gap-1.5 border-t border-border-2 px-2 pb-1 pt-2 text-[10.5px] text-dim">
            <ShieldIcon size={11} />
            {t.detail.share.privacyFooter}
          </div>
        </div>
      )}
    </Popover>
  );
}
