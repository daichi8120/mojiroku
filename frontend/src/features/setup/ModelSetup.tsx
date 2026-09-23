// 初回のモデル準備（#112）。文字起こしモデル（turbo）と無音検出（Silero VAD）が手元に無いとき、
// 何をダウンロードするのかを先に見せて、その場で取得できるようにする。
//
// 以前は最初の文字起こしが黙ってダウンロードを始め、会議モードはモデルが無いとライブ文字起こしを
// 出さないまま「話し始めると表示されます」と言い続けていた（live_stt.rs はモデルが無いと静かに終わる）。
import { useI18n } from "@/i18n";
import { cx } from "@/lib/cx";
import { LIVE_MODEL_FILE, VAD_MODEL_FILE, useModelDownloads, type ModelDownload } from "@/lib/modelDownloads";
import { ProgressBar } from "@/components/ui";
import { CpuIcon } from "@/components/icons";
import { DownloadControl } from "@/features/settings/SettingsView";

const SETUP_FILES = [LIVE_MODEL_FILE, VAD_MODEL_FILE] as const;

/**
 * ライブ文字起こし（と既定の文字起こし）に要るモデルが揃っているか。
 * 一覧がまだ届いていない間は null（その間は何も出さない。準備済みの人に一瞬カードが見えないように）。
 */
export function useLiveModels() {
  const { downloads } = useModelDownloads();
  const models = SETUP_FILES.map((f) => downloads[f]);
  const known = models.every((m) => m !== undefined);
  const ready = known ? models.every((m) => m!.status === "ready") : null;
  return { ready, models: models.filter((m): m is ModelDownload => !!m) };
}

export function ModelSetupCard({
  variant,
  className,
}: {
  /** home: 初回の案内 / meeting: 会議を始める前 / live: 録音中にモデルが無いと分かったとき */
  variant: "home" | "meeting" | "live";
  className?: string;
}) {
  const { t } = useI18n();
  const { ready, models } = useLiveModels();
  if (ready !== false) return null;
  const copy = t.setup[variant];
  const missing = models.filter((m) => m.status !== "ready");
  return (
    <div
      className={cx(
        "rounded-card border border-brand/30 bg-brand/8 px-4 py-3.5",
        variant === "live" && "text-left",
        className,
      )}
    >
      <div className="flex items-start gap-3">
        <span className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-ctl bg-brand/15 text-brand-light">
          <CpuIcon size={16} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-semibold text-ink">{copy.title}</div>
          <p className="mt-0.5 text-[12px] leading-relaxed text-muted">{copy.body}</p>
          <ul className="mt-3 flex flex-col gap-2">
            {missing.map((m) => (
              <li key={m.file} className="flex items-center gap-3">
                <div className="min-w-0 flex-1">
                  <div className="text-[12px] text-sub">
                    {m.file === VAD_MODEL_FILE ? t.setup.vadName : t.setup.whisperName} ·{" "}
                    {formatBytes(m.size_bytes)}
                  </div>
                  {m.status === "downloading" && (
                    <ProgressBar className="mt-1.5" value={m.downloaded_bytes / Math.max(1, m.size_bytes)} />
                  )}
                </div>
                <DownloadControl model={m} />
              </li>
            ))}
          </ul>
        </div>
      </div>
    </div>
  );
}

function formatBytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)} GB`;
  if (n >= 1e6) return `${Math.round(n / 1e6)} MB`;
  return `${(n / 1e6).toFixed(2)} MB`;
}
