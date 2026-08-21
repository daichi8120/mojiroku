// 会議モード（Studio 08）— システム音声＋マイクをローカルキャプチャ + ライブノート。
// 録音状態はアプリ全体（App.tsx / useApp().meeting）が持つので、画面遷移しても継続する。
// この画面は「開始（idle）」と「録音中（capturing/stopping）」の2モードを描画するだけ。
// ライブ AI ノートは未実装（停止後に詳細画面で生成）。モック表示は撤去済み。
import { useEffect, useRef, useState } from "react";
import { useApp } from "@/lib/app";
import {
  checkSystemAudioPermission,
  type LiveLine,
  useMeetingLive,
} from "@/lib/tauri";
import { cx } from "@/lib/cx";
import { useI18n } from "@/i18n";
import { formatTimestamp } from "@/lib/types";
import { Button, ConfirmDialog } from "@/components/ui";
import { PrivacyBar } from "@/components/composite";
import { ShieldIcon, SparklesIcon, StopIcon, VideoIcon } from "@/components/icons";

// システム音声 + マイクのレベルメータ（小さな縦バー）。
const METER_BARS = [6, 11, 8, 13, 5];

export function MeetingView() {
  const { meeting, startMeeting, stopMeeting, discardMeeting } = useApp();
  const { t } = useI18n();
  const capturing = meeting.status === "capturing";
  const stopping = meeting.status === "stopping";

  // 開始画面（idle）の許可プリフライト。録音は開始しない（押した瞬間に録らないのが要件）。
  const [denied, setDenied] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);

  // ライブ文字起こし（増分C）。確定行＋未確定 tail の現在ビュー全体が毎 tick 届く。使い捨て。
  // 画面に戻った直後は空だが、次の tick で現在ビュー一式が再配信されるので自然に復元する。
  const [liveLines, setLiveLines] = useState<LiveLine[]>([]);
  const liveScrollRef = useRef<HTMLDivElement | null>(null);
  useMeetingLive(setLiveLines);

  // idle のときだけ許可状態を確認して開始ボタン/誘導の出し分けに使う。
  useEffect(() => {
    if (meeting.status !== "idle") return;
    let active = true;
    checkSystemAudioPermission()
      .then((g) => {
        if (active) setDenied(!g);
      })
      .catch(() => {
        if (active) setDenied(false);
      });
    return () => {
      active = false;
    };
  }, [meeting.status]);

  // 経過時間は startedAt から算出（遷移して戻っても連続）。1 秒ごとに再描画。
  const [, forceTick] = useState(0);
  useEffect(() => {
    if (meeting.status === "idle") return;
    const t = window.setInterval(() => forceTick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [meeting.status]);
  const elapsed = meeting.startedAt ? Math.floor((Date.now() - meeting.startedAt) / 1000) : 0;

  // 新しい行が来たら最下部へ自動スクロール。
  useEffect(() => {
    const el = liveScrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [liveLines]);

  const begin = async () => {
    const r = await startMeeting();
    if (r === "denied") setDenied(true);
    // started → status=capturing で自動的に録音中ビューへ切替（遷移は不要）。
  };

  // ── 開始画面（idle）。サイドバー/ホームから来た直後はここ。録音はボタンで開始。 ──
  if (meeting.status === "idle") {
    return (
      <div className="flex min-h-full flex-col items-center justify-center px-8 py-12">
        <div className="w-full max-w-[460px] rounded-win border border-border bg-surface p-8 text-center">
          <span className="mx-auto flex h-14 w-14 items-center justify-center rounded-[14px] bg-brand/15 text-brand-light">
            <VideoIcon size={26} />
          </span>
          <h1 className="mt-4 text-[18px] font-bold text-ink">{t.meeting.idle.title}</h1>
          <p className="mt-2 text-[13px] leading-relaxed text-muted">{t.meeting.idle.desc}</p>

          {denied ? (
            <div className="mt-6 rounded-card border border-amber/30 bg-amber/10 px-4 py-3.5 text-left">
              <div className="flex items-center gap-2 text-[12.5px] font-medium text-amber">
                <ShieldIcon size={15} />
                {t.meeting.idle.permTitle}
              </div>
              <p className="mt-1.5 text-[11.5px] leading-relaxed text-muted">
                {t.meeting.idle.permBody}
              </p>
              <button
                onClick={begin}
                className="mt-3 inline-flex h-9 items-center gap-2 rounded-btn border border-border-2 bg-surface-2 px-4 text-[12.5px] font-medium text-ink transition-colors hover:bg-hover"
              >
                {t.meeting.idle.permStart}
              </button>
            </div>
          ) : (
            <button
              onClick={begin}
              className="mt-6 inline-flex h-12 w-full items-center justify-center gap-2.5 rounded-btn text-[14px] font-medium text-white transition-[filter] hover:brightness-110"
              style={{ background: "linear-gradient(180deg,#6366F1,#4F46E5)" }}
            >
              <span className="h-2.5 w-2.5 rounded-full bg-white/90" />
              {t.meeting.idle.start}
            </button>
          )}

          <div className="mt-5">
            <PrivacyBar>{t.meeting.idle.privacy}</PrivacyBar>
          </div>
          <p className="mt-3 text-[11px] leading-relaxed text-faint">
            {t.meeting.idle.headphoneHint}
          </p>
        </div>
      </div>
    );
  }

  // ── 録音中（capturing / stopping） ──
  return (
    <div className="flex h-full min-h-full flex-col bg-bg">
      {/* ヘッダ */}
      <header className="flex items-center justify-between gap-4 border-b border-line px-6 py-3.5">
        <div className="flex min-w-0 items-center gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[9px] bg-brand/15 text-brand-light">
            <VideoIcon size={18} />
          </span>
          <div className="min-w-0">
            <h1 className="truncate text-[15px] font-bold text-ink">{t.meeting.live.title}</h1>
            <div className="mt-0.5 truncate text-[11px] text-muted">
              {t.meeting.live.subtitle}
            </div>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-3.5">
          {/* システム音声 + マイク レベルメータ */}
          <div className="flex items-center gap-2 rounded-btn border border-border-2 bg-surface-2 px-3 py-1.5">
            <span className="text-[11.5px] text-body">
              {capturing ? t.meeting.live.meterCapturing : t.app.meetingBar.saving}
            </span>
            <span className="flex h-3.5 items-end gap-0.5">
              {METER_BARS.map((h, i) => {
                const inactive = i === METER_BARS.length - 1;
                return (
                  <i
                    key={i}
                    className={cx(
                      "w-[2.5px] rounded-full",
                      inactive ? "bg-border-3" : "animate-mjpulse bg-green",
                    )}
                    style={{ height: h, animationDelay: `${i * 140}ms` }}
                  />
                );
              })}
            </span>
          </div>

          {/* 録音タイマー */}
          <div className="flex items-center gap-2">
            <span className="h-2 w-2 animate-mjpulse rounded-full bg-red shadow-[0_0_0_3px_rgba(239,68,68,0.18)]" />
            <span className="font-mono text-[13px] text-ink tnum">
              {formatTimestamp(elapsed * 1000)}
            </span>
          </div>

          {/* 破棄（保存しない） */}
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setConfirmDiscard(true)}
            disabled={stopping}
          >
            {t.meeting.live.discard}
          </Button>

          <Button
            variant="primary"
            size="sm"
            icon={<StopIcon size={14} />}
            className="shadow-[0_10px_26px_rgba(239,68,68,0.35)]"
            style={{ background: "#EF4444" }}
            onClick={() => void stopMeeting()}
            disabled={stopping}
          >
            {stopping ? t.app.meetingBar.saving : t.app.meetingBar.stopAndSave}
          </Button>
        </div>
      </header>

      {/* プライバシーバー */}
      <div className="px-6 py-2">
        <PrivacyBar>{t.meeting.live.privacy}</PrivacyBar>
      </div>

      {/* 2 カラム */}
      <div className="flex min-h-0 flex-1">
        {/* 左: ライブ文字起こし */}
        <section className="flex min-w-0 flex-[1.35] flex-col border-r border-line">
          <div className="flex items-center px-5 pb-2.5 pt-3.5">
            <span className="text-[12px] font-bold tracking-wide text-sub">
              {t.meeting.live.transcriptLabel}
            </span>
          </div>

          <div className="min-h-0 flex-1 overflow-auto px-5 pb-5 pt-1">
            {liveLines.length > 0 ? (
              // 実キャプチャ中: ライブ文字起こし（増分C）。確定行＋未確定 tail を流す。これは
              // 使い捨てプレビューで、保存時に話者分離つきデュアルトラックで作り直す（権威）。
              <div className="flex h-full flex-col">
                <div ref={liveScrollRef} className="min-h-0 flex-1 overflow-auto pr-1">
                  {liveLines.map((l, i) => (
                    <div key={i} className="py-1">
                      <span
                        className={cx(
                          "text-[13.5px] leading-[1.7]",
                          l.committed ? "text-speech" : "text-muted",
                        )}
                      >
                        {l.text}
                      </span>
                      {!l.committed && i === liveLines.length - 1 && (
                        <span className="ml-1 inline-block h-3.5 w-0.5 animate-mjpulse bg-brand align-middle" />
                      )}
                    </div>
                  ))}
                </div>
                <div className="shrink-0 border-t border-line px-1 pt-2 text-[11px] leading-relaxed text-faint">
                  {t.meeting.live.draftFooter}
                </div>
              </div>
            ) : (
              // ウォームアップ（モデルロード中 / まだ発話なし）。
              <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
                <span className="h-2 w-2 animate-mjpulse rounded-full bg-red" />
                <span className="text-[13px] text-body">{t.meeting.live.warmupTitle}</span>
                <span className="text-[11.5px] text-muted">{t.meeting.live.warmupHint}</span>
              </div>
            )}
          </div>
        </section>

        {/* 右: ライブ AI ノート */}
        <aside className="flex min-w-0 flex-1 flex-col bg-surface">
          <div className="flex items-center gap-2 px-[18px] pb-2.5 pt-3.5">
            <span className="flex h-5 w-5 items-center justify-center rounded-md bg-brand text-white">
              <SparklesIcon size={12} />
            </span>
            <span className="text-[12px] font-bold text-ink">{t.meeting.live.aiNotesLabel}</span>
            <span className="ml-auto text-[10.5px] text-dim">{t.meeting.live.aiNotesAfterStop}</span>
          </div>

          {/* ライブ AI ノートは未実装。空白にせず、停止後の実フローを案内する。 */}
          <div className="min-h-0 flex-1 overflow-auto px-[18px] pb-4 pt-3">
            <p className="text-[12.5px] leading-relaxed text-muted">
              {t.meeting.live.aiNotesSoon}
            </p>
            <p className="mt-2.5 text-[12.5px] leading-relaxed text-muted">
              {t.meeting.live.aiNotesDetail}
            </p>
          </div>
        </aside>
      </div>

      <ConfirmDialog
        open={confirmDiscard}
        title={t.meeting.discardConfirm.title}
        body={t.meeting.discardConfirm.body}
        confirmLabel={t.meeting.discardConfirm.confirm}
        onConfirm={() => {
          setConfirmDiscard(false);
          void discardMeeting();
        }}
        onCancel={() => setConfirmDiscard(false)}
      />
    </div>
  );
}
