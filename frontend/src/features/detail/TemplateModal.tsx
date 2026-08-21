// 議事録・要約の生成モーダル（Studio 06・実機能）。
// テンプレ選択 → summarize で生成。生成エンジン（ローカル/クラウド BYOK）は設定（settings.json）に従う。
// ここでは実エンジンを読み取り専用で表示する（実際の送信先を正しく伝えるため）。
import { useEffect, useState, type ReactNode } from "react";
import { getSettings, summarize, useSummarizeProgress } from "@/lib/tauri";
import type { Progress, Settings, Summary, Transcript } from "@/lib/types";
import { useApp } from "@/lib/app";
import { translateError, useI18n } from "@/i18n";
import { cx } from "@/lib/cx";
import { Button, Modal, ModalHeader, ProgressBar } from "@/components/ui";
import { CheckIcon, LayersIcon, MessageIcon, PlusIcon } from "@/components/icons";

const PROVIDER_LABEL: Record<Settings["provider"], string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
};

function TemplateOption({
  icon,
  title,
  desc,
  selected,
  dashed,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  desc: string;
  selected: boolean;
  dashed?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cx(
        "flex w-full items-center gap-3 rounded-[11px] border px-3 py-3 text-left transition-colors",
        selected
          ? "border-brand bg-selected"
          : dashed
            ? "border-dashed border-border-3 hover:bg-hover"
            : "border-border-2 hover:bg-hover",
      )}
    >
      <span
        className={cx(
          "flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-[9px]",
          selected ? "bg-[rgba(99,102,241,0.18)] text-brand-lighter" : "bg-hover text-muted",
        )}
      >
        {icon}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13.5px] font-semibold text-ink">{title}</span>
        <span className="mt-px block text-[11.5px] text-muted">{desc}</span>
      </span>
      {selected && (
        <span className="flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-full bg-brand text-white">
          <CheckIcon size={11} />
        </span>
      )}
    </button>
  );
}

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
  const [templateId, setTemplateId] = useState("minutes");

  // 開いた瞬間に、開いた導線が指定したテンプレへ合わせる（議事録/要約/アクション/再生成）。
  useEffect(() => {
    if (open) setTemplateId(presetTemplate);
  }, [open, presetTemplate]);
  // 生成エンジンは設定（settings.json）が唯一の真実。summarize コマンドが engine を見て
  // ローカル/クラウドへ分岐する。ここでは実エンジンを読み取り専用で表示するだけ。
  const [engine, setEngine] = useState<Settings["engine"]>("local");
  const [provider, setProvider] = useState<Settings["provider"]>("anthropic");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);

  // モーダルを開くたびに最新の設定を読む（設定画面で変更され得るため）。
  useEffect(() => {
    if (!open) return;
    let active = true;
    getSettings()
      .then((s) => {
        if (!active) return;
        setEngine(s.engine);
        setProvider(s.provider);
      })
      .catch(() => {
        /* 取得失敗時はローカル表示のまま（保守的） */
      });
    return () => {
      active = false;
    };
  }, [open]);

  useSummarizeProgress((p) => setProgress(p));

  const isCustom = templateId === "custom";

  const handleClose = () => {
    if (busy) return; // 生成中は閉じない
    setProgress(null);
    onClose();
  };

  const generate = async () => {
    if (isCustom) {
      toast(t.detail.templateModal.customSoonToast, "info");
      return;
    }
    setBusy(true);
    setProgress(null);
    try {
      const summary = await summarize(transcript, recordingId, templateId);
      onCreated(summary);
      toast(t.detail.templateModal.created, "success");
      setProgress(null);
      onClose(); // 成功時は busy ガードを通さず直接閉じる
    } catch (e) {
      toast(translateError(e, t), "error");
    } finally {
      setBusy(false);
    }
  };

  const isCloud = engine === "cloud";
  const engineLabel = isCloud
    ? t.detail.templateModal.engineCloud(PROVIDER_LABEL[provider])
    : t.detail.templateModal.engineLocal;
  const pct = progress && progress.total ? Math.round((progress.done / progress.total) * 100) : 0;
  const progressLabel =
    progress?.stage === "queued"
      ? t.detail.templateModal.progressQueued
      : progress?.stage === "download_llm"
        ? t.detail.templateModal.progressDownload(pct)
        : t.detail.templateModal.progressGenerating(engineLabel);
  const progressValue =
    progress && progress.total ? progress.done / progress.total : progress ? 0.5 : 0.08;

  return (
    <Modal open={open} onClose={handleClose} width={452}>
      <ModalHeader
        title={
          <span className="flex flex-col">
            <span>{t.detail.templateModal.title}</span>
            <span className="text-[11px] font-normal text-muted">
              {t.detail.templateModal.subtitle}
            </span>
          </span>
        }
        onClose={handleClose}
      />

      <div className="flex flex-col gap-2 px-4 py-3.5">
        <TemplateOption
          selected={templateId === "minutes"}
          onClick={() => setTemplateId("minutes")}
          icon={<LayersIcon size={17} />}
          title={t.detail.templateModal.templates.minutes.title}
          desc={t.detail.templateModal.templates.minutes.desc}
        />
        <TemplateOption
          selected={templateId === "summary"}
          onClick={() => setTemplateId("summary")}
          icon={<MessageIcon size={17} />}
          title={t.detail.templateModal.templates.summary.title}
          desc={t.detail.templateModal.templates.summary.desc}
        />
        <TemplateOption
          selected={templateId === "action_items"}
          onClick={() => setTemplateId("action_items")}
          icon={<CheckIcon size={17} />}
          title={t.detail.templateModal.templates.actionItems.title}
          desc={t.detail.templateModal.templates.actionItems.desc}
        />
        <TemplateOption
          dashed
          selected={isCustom}
          onClick={() => setTemplateId("custom")}
          icon={<PlusIcon size={17} />}
          title={t.detail.templateModal.templates.custom.title}
          desc={t.detail.templateModal.templates.custom.desc}
        />
        {isCustom && (
          <div className="flex flex-col gap-1.5">
            <textarea
              disabled
              placeholder={t.detail.templateModal.customPlaceholder}
              className="h-20 w-full resize-none rounded-[10px] border border-border-2 bg-surface-2 px-3 py-2 text-[12.5px] text-body placeholder:text-dim disabled:opacity-70"
            />
            <p className="text-[11px] text-amber">{t.detail.templateModal.customSoonNote}</p>
          </div>
        )}
      </div>

      <div className="px-4 pb-3.5">
        <div className="mb-2 text-[11px] font-bold uppercase tracking-[0.06em] text-dim">
          {t.detail.templateModal.engineSection}
        </div>
        {isCloud ? (
          <div className="rounded-[10px] border border-amber/30 bg-amber/10 px-3 py-2.5">
            <div className="flex items-center gap-2">
              <span className="h-2 w-2 shrink-0 rounded-full bg-amber" />
              <span className="text-[12.5px] font-semibold text-ink">
                {t.detail.templateModal.cloudBadge(PROVIDER_LABEL[provider])}
              </span>
            </div>
            <p className="mt-1.5 text-[11px] text-amber">
              {t.detail.templateModal.cloudWarn(PROVIDER_LABEL[provider])}
            </p>
          </div>
        ) : (
          <div className="rounded-[10px] border border-border-2 bg-surface-2 px-3 py-2.5">
            <div className="flex items-center gap-2">
              <span className="h-2 w-2 shrink-0 rounded-full bg-green" />
              <span className="text-[12.5px] font-semibold text-ink">
                {t.detail.templateModal.localBadge}
              </span>
            </div>
            <p className="mt-1.5 text-[11px] text-green">{t.detail.templateModal.localNote}</p>
          </div>
        )}
        <p className="mt-1.5 text-[10.5px] text-faint">{t.detail.templateModal.engineHint}</p>
      </div>

      <div className="flex items-center justify-between gap-3 border-t border-border px-4 py-3.5">
        {busy ? (
          <div className="min-w-0 flex-1">
            <div className="mb-1.5 text-[11px] text-sub">{progressLabel}</div>
            <ProgressBar value={progressValue} tone="green" />
          </div>
        ) : (
          <span className="text-[11px] text-dim">
            {isCloud
              ? t.detail.templateModal.footerCloud(PROVIDER_LABEL[provider])
              : t.detail.templateModal.footerLocal}
          </span>
        )}
        <Button variant="primary" onClick={generate} disabled={busy || isCustom}>
          {busy ? t.detail.templateModal.generating : t.detail.templateModal.generate}
        </Button>
      </div>
    </Modal>
  );
}
