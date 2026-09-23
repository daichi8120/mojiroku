// 議事録・要約の生成モーダル（Studio 06・実機能）。
// テンプレ選択 → summarize で生成。生成エンジン（ローカル/クラウド BYOK）は設定（settings.json）に従う。
// ここでは実エンジンを読み取り専用で表示する（実際の送信先を正しく伝えるため）。
import { useEffect, useRef, useState, type ReactNode } from "react";
import { getSettings, summarize, useSummarizeProgress } from "@/lib/tauri";
import type { Progress, Settings, Summary, Transcript } from "@/lib/types";
import { useApp } from "@/lib/app";
import { translateError, useI18n } from "@/i18n";
import { Button, Modal, ModalHeader, ProgressBar } from "@/components/ui";
import { CheckIcon, LayersIcon, MessageIcon } from "@/components/icons";

const PROVIDER_LABEL: Record<Settings["provider"], string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
};

const TEMPLATE_ICON: Record<string, ReactNode> = {
  minutes: <LayersIcon size={17} />,
  summary: <MessageIcon size={17} />,
  action_items: <CheckIcon size={17} />,
};

/**
 * 議事録・要約・アクションアイテムの生成ダイアログ（#107）。
 *
 * テンプレは開いた導線（右パネルのボタン / 空状態 / 再生成）が決める。以前はここでもう一度
 * 選ばせていたが、同じ選択を二度させるだけだった。
 * - ローカル: 設定を読み終えたらそのまま生成を始め、進捗を出す。
 * - クラウド（BYOK）: 文字起こしが外部へ送られるので、警告を見せてボタンを押すまで始めない。
 *   設定が読めなかったときも同じ扱い（どちらのエンジンか分からないまま送らない）。
 */
export function TemplateModal({
  open,
  onClose,
  recordingId,
  transcript,
  onCreated,
  presetTemplate = "minutes",
}: {
  open: boolean;
  onClose: () => void;
  recordingId: string;
  transcript: Transcript;
  onCreated: (summary: Summary) => void;
  presetTemplate?: string;
}) {
  const { toast } = useApp();
  const { t } = useI18n();
  const tm = t.detail.templateModal;
  const templateId = presetTemplate;
  // 生成エンジンは設定（settings.json）が唯一の真実。summarize コマンドが engine を見て
  // ローカル/クラウドへ分岐する。ここでは実エンジンを読み取り専用で表示するだけ。
  const [engine, setEngine] = useState<Settings["engine"] | null>(null);
  const [provider, setProvider] = useState<Settings["provider"]>("anthropic");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const startedRef = useRef(false);

  // モーダルを開くたびに最新の設定を読む（設定画面で変更され得るため）。
  useEffect(() => {
    if (!open) return;
    startedRef.current = false;
    setEngine(null);
    setError(null);
    let active = true;
    getSettings()
      .then((s) => {
        if (!active) return;
        setProvider(s.provider);
        setEngine(s.engine);
      })
      .catch(() => {
        // 読めないときはクラウド扱い（自動では始めない）。
        if (active) setEngine("cloud");
      });
    return () => {
      active = false;
    };
  }, [open]);

  useSummarizeProgress((p) => setProgress(p));

  const handleClose = () => {
    if (busy) return; // 生成中は閉じない
    setProgress(null);
    onClose();
  };

  const generate = async () => {
    startedRef.current = true;
    setBusy(true);
    setError(null);
    setProgress(null);
    try {
      const summary = await summarize(transcript, recordingId, templateId);
      onCreated(summary);
      toast(tm.created, "success");
      setProgress(null);
      onClose(); // 成功時は busy ガードを通さず直接閉じる
    } catch (e) {
      setError(translateError(e, t));
    } finally {
      setBusy(false);
    }
  };

  // ローカルは開いたらすぐ始める（1 回だけ。失敗後の再試行はボタンで）。
  useEffect(() => {
    if (open && engine === "local" && !startedRef.current) void generate();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, engine]);

  const isCloud = engine === "cloud";
  const engineLabel = isCloud ? tm.engineCloud(PROVIDER_LABEL[provider]) : tm.engineLocal;
  const pct = progress && progress.total ? Math.round((progress.done / progress.total) * 100) : 0;
  const progressLabel =
    progress?.stage === "queued"
      ? tm.progressQueued
      : progress?.stage === "download_llm"
        ? tm.progressDownload(pct)
        : tm.progressGenerating(engineLabel);
  const progressValue =
    progress && progress.total ? progress.done / progress.total : progress ? 0.5 : 0.08;
  const label = templateLabelFor(templateId, tm.templates);

  return (
    <Modal open={open} onClose={handleClose} width={420}>
      <ModalHeader
        title={
          <span className="flex items-center gap-2.5">
            <span className="flex h-8 w-8 items-center justify-center rounded-ctl bg-brand/15 text-brand-light">
              {TEMPLATE_ICON[templateId] ?? <LayersIcon size={17} />}
            </span>
            <span className="flex flex-col">
              <span>{tm.titleFor(label.title)}</span>
              <span className="text-[12px] font-normal text-muted">{label.desc}</span>
            </span>
          </span>
        }
        onClose={handleClose}
      />

      <div className="px-5 py-4">
        {isCloud ? (
          <div className="rounded-btn border border-amber/30 bg-amber/10 px-3 py-2.5">
            <div className="flex items-center gap-2">
              <span className="h-2 w-2 shrink-0 rounded-full bg-amber" />
              <span className="text-[13px] font-semibold text-ink">
                {tm.cloudBadge(PROVIDER_LABEL[provider])}
              </span>
            </div>
            <p className="mt-1.5 text-[12px] text-amber">{tm.cloudWarn(PROVIDER_LABEL[provider])}</p>
          </div>
        ) : (
          <div className="flex items-center gap-2 text-[12px] text-muted">
            <span className="h-2 w-2 shrink-0 rounded-full bg-green" />
            {tm.localBadge} · {tm.localNote}
          </div>
        )}

        {busy && (
          <div className="mt-4">
            <div className="mb-1.5 text-[12px] text-sub">{progressLabel}</div>
            <ProgressBar value={progressValue} tone="green" />
          </div>
        )}
        {error && !busy && (
          <p role="alert" className="mt-4 rounded-btn border border-red/40 bg-red/8 px-3 py-2 text-[13px] text-red-light">
            {error}
          </p>
        )}
        <p className="mt-3 text-[11px] text-faint">{tm.engineHint}</p>
      </div>

      {(isCloud || error) && !busy && (
        <div className="flex justify-end gap-2 border-t border-border px-5 py-3.5">
          <Button variant="secondary" size="sm" onClick={handleClose}>
            {t.common.cancel}
          </Button>
          <Button variant="primary" size="sm" onClick={() => void generate()}>
            {error ? t.common.retry : tm.sendAndGenerate(PROVIDER_LABEL[provider])}
          </Button>
        </div>
      )}
    </Modal>
  );
}

type TemplateCopy = Record<"minutes" | "summary" | "actionItems", { title: string; desc: string }>;

function templateLabelFor(id: string, copy: TemplateCopy) {
  if (id === "summary") return copy.summary;
  if (id === "action_items") return copy.actionItems;
  return copy.minutes;
}
