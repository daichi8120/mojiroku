// 録音の詳細（Studio 04 + 06 + 10 + 11）。最も大きいビュー。
// 中央メイン（AI議事録 + 文字起こし/チャプター）+ 右ペイン 222px（話者 + MCP）。
// 左サイドバーは App が描く。実機能: 取得 / 話者改名 / 要約生成 / 共有。
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { getJobStart, getStageStart, markJobStart } from "@/lib/jobClock";
import { useApp } from "@/lib/app";
import { cx } from "@/lib/cx";
import {
  cancelJob,
  deleteRecording,
  diarizeRecording,
  getRecording,
  listJobs,
  recordingAudioSrc,
  renameRecording,
  transcribeRecording,
  useJobUpdate,
  setSegmentSpeaker,
} from "@/lib/tauri";
import { MOCK_PREVIEW } from "@/lib/mockData";
import { translateError, useI18n } from "@/i18n";
import {
  formatDateTime,
  formatDuration,
  type Job,
  type RecordingDetail,
  type Summary,
  type Segment,
  formatTimestamp,
  speakerChipStyle,
  speakerName,
} from "@/lib/types";
import { ConfirmDialog, MenuItem, Modal, ModalHeader, Spinner, Toggle } from "@/components/ui";
import { EmptyState, PreviewTag, TranscriptList, Waveform } from "@/components/composite";
import {
  ArrowUpRightIcon,
  BotIcon,
  CheckIcon,
  PencilIcon,
  PlayIcon,
  RefreshIcon,
  SparklesIcon,
  TrashIcon,
  XIcon,
} from "@/components/icons";
import { SpeakerPanel } from "./SpeakerPanel";
import { SharePopover } from "./SharePopover";
import { TemplateModal } from "./TemplateModal";
import { AskDrawer } from "./AskDrawer";
import { AudioPlayer } from "./AudioPlayer";

// チャプターはモック（トピック自動分割は未実装・Studio 15）。
const CHAPTERS = [
  { time: "00:00", color: "#6366F1", grow: 2.1, title: "ベータ配布の状況", dur: "2分", body: "未署名ビルドの初回起動でつまずく人が多く、許可手順に画像を追加。" },
  { time: "02:10", color: "#22D3EE", grow: 3.3, title: "オンボーディング刷新", dur: "3.5分", body: "「ローカル完結・基本無料」を初回画面の主役に据える方針で合意。" },
  { time: "05:40", color: "#34D399", grow: 3.6, title: "モデルDLの統合", dur: "3.6分", body: "ダウンロードを初回フローへ統合。進捗は控えめに、状態は分かるように。" },
  { time: "09:20", color: "#FCD34D", grow: 3.1, title: "ネクストと宿題の確認", dur: "3.1分", body: "佐藤=初回画面デザイン案、鈴木=DL進捗UI。次回までに共有。" },
];

const MOCK_TRANSLATION = "（翻訳プレビュー）この発話の日本語訳がここに表示されます。";

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={cx(
        "-mb-px border-b-2 px-3 pb-2 pt-1 text-[13px] transition-colors",
        active ? "border-brand text-ink" : "border-transparent text-muted hover:text-body",
      )}
    >
      {children}
    </button>
  );
}

export function DetailView({ id }: { id: string }) {
  const { navigate, toast, refreshRecents } = useApp();
  const { t, lang } = useI18n();
  // 詳細画面の UI ラベル（テンプレ id → 表示名）。未知 id は id をそのまま出す。
  const templateLabel = (id: string): string => t.detail.templateNames[id] ?? id;
  const [detail, setDetail] = useState<RecordingDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [audioSrc, setAudioSrc] = useState<string | null>(null);

  const [tab, setTab] = useState<"transcript" | "chapters">("transcript");
  const [translateOn, setTranslateOn] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [presetTemplate, setPresetTemplate] = useState("minutes");
  const [askOpen, setAskOpen] = useState(false);
  const [confirmDel, setConfirmDel] = useState(false);
  const [deleting, setDeleting] = useState(false);
  // タイトルのインライン編集。
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleValue, setTitleValue] = useState("");
  const [savingTitle, setSavingTitle] = useState(false);

  // バックグラウンドジョブ（ADR-0024）。detail.active_job を起点に job://update で追う。
  const [job, setJob] = useState<Job | null>(null);
  const [jobProgress, setJobProgress] = useState<{ done: number; total: number | null }>({
    done: 0,
    total: null,
  });
  const [transcribeDiarize, setTranscribeDiarize] = useState(false);
  const [starting, setStarting] = useState(false);
  const processing = job?.status === "pending" || job?.status === "running";
  const jobFailed = job?.status === "failed";
  // 実処理中（順番待ち=pending/queued を除く）だけ「ローディングが進んでいる」実感のため
  // 経過時間を刻む。core は多くの段で incremental % を出さない（stage,0,None のみ）ので、
  // 経過時間が「止まっていない」ことの唯一の可視シグナルになる（ETA/% は別途）。
  const jobActive = job?.status === "running" && job.stage !== "queued";
  // 実処理中は 1 秒ごとに now を刻み、経過時間と ETA（残り約N分）を導出する。now を state に
  // 持つことで、経過は jobClock の起点から、ETA はステージ実進捗アンカーから、同じ now で一貫して
  // 計算できる（起点は jobClock 側に持つので画面遷移で remount してもリセットしない）。
  const [nowMs, setNowMs] = useState(0);

  // 指定テンプレへ preset してモーダルを開く（AIで作成グループ / 再生成 / 空状態 共通）。
  const openModal = (templateId: string) => {
    setPresetTemplate(templateId);
    setModalOpen(true);
  };

  // 本文/話者/音声を取り直す（ジョブ完了後の反映用。UI 状態＝タブ等はリセットしない）。
  const reloadDetail = useCallback(() => {
    getRecording(id)
      .then((d) => {
        if (d) {
          setDetail(d);
          setJob(d.active_job ?? null);
        }
      })
      .catch(() => {
        /* 再取得失敗は無視（既存表示を保つ） */
      });
    recordingAudioSrc(id)
      .then(setAudioSrc)
      .catch(() => {});
  }, [id]);

  // マウント時の取りこぼし対策（advisor）: 詳細取得〜job://update 購読の隙間に、fast-fail
  // （no_audio / no_transcript 等）の failed が飛ぶと「処理中」で固まる。active_job を
  // 抱えたら一度だけ listJobs で突き合わせて決着を取り直す。
  const reconcileActiveJob = useCallback(
    (jobId: string) => {
      listJobs()
        .then((jobs) => {
          const mine = jobs.find((j) => j.id === jobId);
          // 一覧（pending/running/failed）に居なければ done/canceled で決着済み → 取り直す。
          if (!mine) reloadDetail();
          else setJob(mine);
        })
        .catch(() => {});
    },
    [reloadDetail],
  );

  // job://update 購読。**自分（recording_id===id）宛のみ**反映する（要石: 相関付け）。
  // 完了/失敗トースト・サイドバー更新は App が担うので、ここは自分のビュー更新だけに徹する。
  useJobUpdate((u) => {
    if (u.recording_id !== id) return;
    if (u.status === "done") {
      setJob(null);
      setJobProgress({ done: 0, total: null });
      reloadDetail();
    } else if (u.status === "canceled") {
      setJob(null);
      setJobProgress({ done: 0, total: null });
    } else if (u.status === "failed") {
      setJob((prev) => (prev ? { ...prev, status: "failed", error: u.error } : prev));
    } else {
      // pending / running。stage/進捗を反映（prev が無ければイベントから合成）。
      setJob((prev) => ({
        id: u.job_id,
        recording_id: u.recording_id,
        kind: u.kind,
        status: u.status,
        params: prev?.params ?? { diarize: false, stt_lang: null, lang },
        stage: u.stage ?? prev?.stage ?? null,
        error: null,
        created_at: prev?.created_at ?? "",
        updated_at: prev?.updated_at ?? "",
      }));
      setJobProgress({ done: u.done, total: u.total });
    }
  });

  // 経過/ETA タイマー: 1 秒ごとに now を更新するだけ（起点は jobClock が保持）。
  useEffect(() => {
    if (!jobActive || !job) {
      setNowMs(0);
      return;
    }
    markJobStart(job.id); // App 未観測でも自分で種を蒔く（冪等）
    const tick = () => setNowMs(Date.now());
    tick();
    const h = window.setInterval(tick, 1000);
    return () => window.clearInterval(h);
  }, [jobActive, job?.id, job?.stage]);

  // now を起点/アンカーと突き合わせて経過と ETA を導出（render で毎秒再計算）。
  const jobStart = jobActive && job ? getJobStart(job.id) : undefined;
  const elapsedSec = jobStart && nowMs ? Math.max(0, Math.floor((nowMs - jobStart) / 1000)) : 0;
  // ETA は total 付き進捗（＝単一ファイル文字起こしの whisper 0-100%）でのみ出す。会議/話者分離は
  // total が来ない（core が %を出さない）ので自動的に経過時間だけになる。ステージ実進捗アンカーから
  // 線形外挿し、pct が十分に進んで(>5%)かつ未完了のときだけ表示（序盤の暴れと 100% 近傍の 0 を避ける）。
  const pctFrac = jobProgress.total ? jobProgress.done / jobProgress.total : 0;
  const stageStart = jobActive && job?.stage ? getStageStart(job.id, job.stage) : undefined;
  const etaMin =
    stageStart && nowMs && pctFrac > 0.05 && pctFrac < 1
      ? Math.max(1, Math.ceil((((nowMs - stageStart) / 1000) * (1 - pctFrac)) / pctFrac / 60))
      : null;

  // 文字起こしジョブ投入（空 transcript の録音・再文字起こし）。
  const startTranscribe = async () => {
    if (starting || processing) return;
    setStarting(true);
    try {
      const res = await transcribeRecording(id, transcribeDiarize);
      // transcribe_recording は必ずジョブを積む（job_id は非 null）が、型上 optional なので楽観 seed は
      // job_id があるときだけ（無ければ即続く job://update が本物の id で state を埋める）。
      if (res.job_id) {
        setJob({
          id: res.job_id,
          recording_id: id,
          kind: "transcribe",
          status: "pending",
          params: { diarize: transcribeDiarize, stt_lang: null, lang },
          stage: null,
          error: null,
          created_at: "",
          updated_at: "",
        });
        setJobProgress({ done: 0, total: null });
      }
    } catch (e) {
      toast(translateError(e, t), "error");
    } finally {
      setStarting(false);
    }
  };

  // 後付け話者分離ジョブ投入（transcript 済み・話者未割当の File/Mic）。
  const startDiarize = async () => {
    if (starting || processing) return;
    setStarting(true);
    try {
      const res = await diarizeRecording(id);
      if (res.job_id) {
        setJob({
          id: res.job_id,
          recording_id: id,
          kind: "diarize",
          status: "pending",
          params: { diarize: false, stt_lang: null, lang },
          stage: null,
          error: null,
          created_at: "",
          updated_at: "",
        });
        setJobProgress({ done: 0, total: null });
      }
    } catch (e) {
      toast(translateError(e, t), "error");
    } finally {
      setStarting(false);
    }
  };

  // 順番待ち（pending）のジョブをキャンセル（running は完走）。
  const onCancelJob = async () => {
    if (!job) return;
    try {
      const ok = await cancelJob(job.id);
      if (ok) {
        setJob(null);
        setJobProgress({ done: 0, total: null });
        refreshRecents();
      }
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };

  // stage キー → 表示名（job.stages。未知キーは transcribe へフォールバック）。
  const stageLabel = (stage: string | null): string =>
    (stage && t.job.stages[stage]) || t.job.stages.transcribe;

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setTab("transcript");
    setTranslateOn(false);
    setModalOpen(false);
    setAskOpen(false);
    setConfirmDel(false);
    setEditingTitle(false);
    // 訂正モーダルも必ず捨てる。残すと「別録音へ遷移したのにモーダルだけ前の録音の
    // 発言を掴んでいる」状態になり、選んだ瞬間に**別録音の無関係な発言**が書き換わる
    // （話者候補は新録音のものなので、コア側の話者検証も素通りしてしまう）。
    setFixingSeg(null);
    setAudioSrc(null);
    setJob(null);
    setJobProgress({ done: 0, total: null });
    setTranscribeDiarize(false);
    // 再生用の音声 URL（無くても本体表示は止めない・best-effort）。
    recordingAudioSrc(id)
      .then((src) => {
        if (!cancelled) setAudioSrc(src);
      })
      .catch(() => {
        /* 音声が無くても致命的でない */
      });
    getRecording(id)
      .then((d) => {
        if (cancelled) return;
        if (!d) {
          navigate({ view: "history" });
          return;
        }
        setDetail(d);
        setLoading(false);
        // 処理中で開く（active_job）。取りこぼし対策で一度だけ突き合わせる。
        if (d.active_job) {
          setJob(d.active_job);
          reconcileActiveJob(d.active_job.id);
        }
      })
      .catch((e) => {
        if (cancelled) return;
        // 原文（キー）のまま保持し、表示時に translateError する（言語切替にも追従させる）。
        setError(String(e));
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [id, navigate, reconcileActiveJob]);

  // 同じテンプレの要約は置換（upsert）。重複カードを増やさない。
  const onCreated = (summary: Summary) =>
    setDetail((prev) => {
      if (!prev) return prev;
      const idx = prev.summaries.findIndex((s) => s.template_id === summary.template_id);
      const summaries =
        idx >= 0
          ? prev.summaries.map((s, i) => (i === idx ? summary : s))
          : [...prev.summaries, summary];
      return { ...prev, summaries };
    });

  // 発言単位の話者訂正（Issue #19）。チップを押すと、その発言だけ話者を選び直せる。
  // 改名（onRenamed）がクラスタ全体を変えるのに対し、こちらは 1 行だけを動かす。
  const [fixingSeg, setFixingSeg] = useState<Segment | null>(null);

  const fixSegmentSpeaker = async (segIdx: number, speakerId: string | null) => {
    // 掴んでいる detail が本当にこの id のものかを確かめてから書く（上の state リセットと
    // 二重化）。state が残る経路を将来また作っても、ここで止まる。
    if (!detail || detail.recording.id !== id) {
      setFixingSeg(null);
      return;
    }
    setFixingSeg(null);
    try {
      await setSegmentSpeaker(id, segIdx, speakerId);
    } catch (e) {
      toast(translateError(e, t), "error");
      return;
    }
    // 再取得せずローカル state をパッチする（改名・タイトル変更と同じ作法）。
    // 要約はコア側で stale が立つので、こちらでも印を合わせる。
    setDetail((prev) =>
      prev
        ? {
            ...prev,
            transcript: {
              ...prev.transcript,
              segments: prev.transcript.segments.map((x) =>
                x.idx === segIdx ? { ...x, speaker_id: speakerId } : x,
              ),
            },
            summaries: prev.summaries?.map((x) => ({ ...x, stale: true })),
          }
        : prev,
    );
    toast(t.composite.speakerFixed, "success");
  };

  const onRenamed = (speakerId: string, displayName: string | null) =>
    setDetail((prev) =>
      prev
        ? {
            ...prev,
            speakers: prev.speakers?.map((s) =>
              s.id === speakerId ? { ...s, display_name: displayName } : s,
            ),
          }
        : prev,
    );

  // 削除（取り消し不可 → 確認後）。成功したら履歴へ戻り、サイドバー最近も更新。
  const onDelete = async () => {
    setDeleting(true);
    try {
      await deleteRecording(id);
      refreshRecents();
      toast(t.history.deleted, "success");
      navigate({ view: "history" });
    } catch (e) {
      toast(translateError(e, t), "error");
      setDeleting(false);
    }
  };

  // タイトル編集開始（現在の生タイトルを初期値に）。
  const beginEditTitle = () => {
    setTitleValue(detail?.recording.title ?? "");
    setEditingTitle(true);
  };

  // タイトル保存（null/空白で既定の「無題」へ）。成功したら表示とサイドバー最近を更新。
  const saveTitle = async () => {
    if (savingTitle) return;
    setSavingTitle(true);
    try {
      await renameRecording(id, titleValue);
      const next = titleValue.trim() || null;
      setDetail((d) => (d ? { ...d, recording: { ...d.recording, title: next } } : d));
      setEditingTitle(false);
      refreshRecents();
      toast(t.history.renamed, "success");
    } catch (e) {
      toast(translateError(e, t), "error");
    } finally {
      setSavingTitle(false);
    }
  };

  if (loading) {
    return (
      <div className="flex min-h-full items-center justify-center">
        <Spinner size={22} />
      </div>
    );
  }

  if (error || !detail) {
    return (
      <div className="flex min-h-full flex-col items-center justify-center gap-3 px-8 text-center">
        <div className="text-[14px] text-sub">{t.detail.loadFailed}</div>
        {error && (
          <div className="max-w-md text-[12px] text-muted">{translateError(error, t)}</div>
        )}
        <button
          onClick={() => navigate({ view: "history" })}
          className="mt-1 text-[12.5px] text-brand-light hover:text-brand-lighter"
        >
          {t.detail.backToHistory}
        </button>
      </div>
    );
  }

  const rec = detail.recording;
  const title = rec.title?.trim() || t.common.untitledRecording;
  const meta = [formatDateTime(rec.created_at, lang), formatDuration(rec.duration_ms)];
  const speakers = detail.speakers ?? [];
  const hasTranscript = detail.transcript.segments.length > 0;
  // 後付けアクションの可否（ADR-0024）。処理中は隠す。会議は録音時に話者付与済み＝diarize 不可。
  const canTranscribe = !processing && !hasTranscript;
  const canDiarize =
    !processing && hasTranscript && speakers.length === 0 && rec.source_type !== "live";

  return (
    <div className="flex h-full min-h-0">
      {/* 中央メイン */}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="shrink-0 border-b border-line px-6 pb-3.5 pt-[18px]">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              {editingTitle ? (
                <div className="flex items-center gap-2">
                  <input
                    autoFocus
                    value={titleValue}
                    onChange={(e) => setTitleValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        void saveTitle();
                      } else if (e.key === "Escape") {
                        e.preventDefault();
                        setEditingTitle(false);
                      }
                    }}
                    placeholder={t.common.untitledRecording}
                    className="min-w-0 flex-1 rounded-[7px] border border-border-3 bg-surface-2 px-2.5 py-1 text-[18px] font-bold text-ink outline-none focus:border-brand"
                  />
                  <button
                    onClick={() => void saveTitle()}
                    disabled={savingTitle}
                    aria-label={t.common.save}
                    title={t.common.save}
                    className="flex h-7 w-7 shrink-0 items-center justify-center rounded-[7px] text-green transition-colors hover:bg-surface-2 disabled:opacity-50"
                  >
                    {savingTitle ? <Spinner size={14} /> : <CheckIcon size={16} />}
                  </button>
                  <button
                    onClick={() => setEditingTitle(false)}
                    disabled={savingTitle}
                    aria-label={t.common.cancel}
                    title={t.common.cancel}
                    className="flex h-7 w-7 shrink-0 items-center justify-center rounded-[7px] text-dim transition-colors hover:bg-surface-2 hover:text-body disabled:opacity-50"
                  >
                    <XIcon size={15} />
                  </button>
                </div>
              ) : (
                <div className="group/title flex items-center gap-2">
                  <h1 className="truncate text-[19px] font-bold text-ink">{title}</h1>
                  <button
                    onClick={beginEditTitle}
                    aria-label={t.history.renameTitle}
                    title={t.history.renameTitle}
                    className="flex h-7 w-7 shrink-0 items-center justify-center rounded-[7px] text-dim opacity-0 transition-all hover:bg-surface-2 hover:text-body group-hover/title:opacity-100"
                  >
                    <PencilIcon size={14} />
                  </button>
                </div>
              )}
              <div className="mt-0.5 font-mono text-[12px] text-faint">{meta.join(" · ")}</div>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              {/* 質問する = ローカル RAG（未実装）のモック。配布時(MOCK_PREVIEW=false)は隠す。 */}
              {MOCK_PREVIEW && (
                <button
                  onClick={() => setAskOpen(true)}
                  className="inline-flex h-7 items-center gap-1.5 rounded-[7px] border border-border-3 bg-surface-2 px-2.5 text-[11.5px] text-body transition-colors hover:bg-hover"
                >
                  <SparklesIcon size={13} className="text-brand-lighter" />
                  質問する
                </button>
              )}
              <SharePopover detail={detail} />
              <button
                onClick={() => setConfirmDel(true)}
                aria-label={t.common.delete}
                title={t.common.delete}
                className="inline-flex h-7 w-7 items-center justify-center rounded-[7px] text-dim transition-colors hover:bg-[rgba(239,68,68,0.12)] hover:text-red-light"
              >
                <TrashIcon size={15} />
              </button>
            </div>
          </div>

          {/* 再生バー。原本があれば実再生（File/Mic/会議の結合 <id>.wav）、無ければ控えめな装飾。 */}
          <div className="mt-3.5">
            {audioSrc ? (
              <AudioPlayer src={audioSrc} fallbackDurationMs={rec.duration_ms} />
            ) : (
              <>
                <div className="flex cursor-default items-center gap-3">
                  <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-border-2 bg-surface-2 text-muted">
                    <PlayIcon size={15} />
                  </div>
                  <Waveform active={false} bars={30} height={42} className="flex-1" />
                  <span className="shrink-0 font-mono text-[11.5px] text-muted tnum">
                    0:00 / {formatDuration(rec.duration_ms)}
                  </span>
                </div>
                <div className="mt-1 text-right text-[10.5px] text-dim">
                  {t.detail.noAudio}
                </div>
              </>
            )}
          </div>
        </header>

        <div className="flex-1 overflow-y-auto px-6 py-4">
          {/* 処理中（ADR-0024）: ステージ + 進捗。pending はキャンセル可（running は完走）。 */}
          {processing && job && (
            <div className="mb-4 rounded-card border border-border-2 bg-surface-2 px-4 py-3.5">
              <div className="flex items-center gap-3">
                <Spinner size={18} />
                <div className="min-w-0 flex-1">
                  <div className="text-[13px] font-semibold text-ink">{t.job.processing}</div>
                  <div className="mt-0.5 text-[12px] text-muted">
                    {/* pending（まだワーカーが取っていない＝重い処理は同時 1 本なので前段の完了待ち）と
                        running+stage=queued（permit 待ち）は「順番待ち」。stage=null の pending を
                        stageLabel の transcribe フォールバックで「文字起こし」と誤表示しない（ADR-0024）。 */}
                    {job.status === "pending" || job.stage === "queued"
                      ? t.job.queued
                      : stageLabel(job.stage)}
                    {jobProgress.total
                      ? ` · ${Math.min(100, Math.round((jobProgress.done / jobProgress.total) * 100))}%`
                      : ""}
                    {jobActive ? ` · ${t.job.elapsed} ${formatDuration(elapsedSec * 1000)}` : ""}
                    {etaMin != null ? ` · ${t.job.remaining(etaMin)}` : ""}
                  </div>
                </div>
                {job.status === "pending" && (
                  <button
                    onClick={() => void onCancelJob()}
                    className="shrink-0 rounded-[7px] border border-border-3 px-2.5 py-1 text-[11.5px] text-muted transition-colors hover:bg-hover hover:text-body"
                  >
                    {t.job.cancel}
                  </button>
                )}
              </div>
              {jobProgress.total ? (
                <div className="mt-2.5 h-1 overflow-hidden rounded-full bg-[rgba(255,255,255,0.08)]">
                  <div
                    className="h-full rounded-full bg-brand transition-[width]"
                    style={{
                      width: `${Math.min(100, Math.round((jobProgress.done / jobProgress.total) * 100))}%`,
                    }}
                  />
                </div>
              ) : null}
            </div>
          )}

          {/* 失敗（ADR-0024）: キー化メッセージを翻訳表示。下の実行ボタンで再試行できる。 */}
          {jobFailed && job && (
            <div className="mb-4 rounded-card border border-red/40 bg-[rgba(239,68,68,0.08)] px-4 py-3 text-[12.5px] text-red-light">
              {job.error ? translateError(job.error, t) : t.job.failedToast}
            </div>
          )}

          {/* 後付け文字起こし（空 transcript の録音）。 */}
          {canTranscribe && (
            <div className="mb-4 rounded-card border border-border-2 bg-surface-2 px-4 py-4">
              <div className="text-[13.5px] font-semibold text-ink">{t.detail.runTranscribe}</div>
              <div className="mt-0.5 text-[12px] text-muted">{t.detail.runTranscribeDesc}</div>
              <div className="mt-3 flex items-center gap-2 text-[12px] text-body">
                <Toggle
                  checked={transcribeDiarize}
                  onChange={setTranscribeDiarize}
                  label={t.detail.runTranscribeDiarize}
                />
                <span>{t.detail.runTranscribeDiarize}</span>
              </div>
              <button
                onClick={() => void startTranscribe()}
                disabled={starting}
                className="mt-3 h-9 rounded-[8px] bg-brand px-4 text-[12.5px] font-semibold text-white transition-[filter] hover:brightness-110 disabled:opacity-50"
              >
                {starting ? <Spinner size={14} /> : t.detail.runTranscribe}
              </button>
            </div>
          )}

          {/* 後付け話者分離（transcript 済み・話者未割当の File/Mic）。 */}
          {canDiarize && (
            <div className="mb-4 flex items-center justify-between gap-3 rounded-card border border-border-2 bg-surface-2 px-4 py-3.5">
              <div className="min-w-0">
                <div className="text-[13px] font-semibold text-ink">{t.detail.runDiarize}</div>
                <div className="mt-0.5 text-[12px] text-muted">{t.detail.runDiarizeDesc}</div>
              </div>
              <button
                onClick={() => void startDiarize()}
                disabled={starting}
                className="inline-flex h-9 shrink-0 items-center gap-1.5 rounded-[8px] border border-border-2 px-3.5 text-[12.5px] font-medium text-body transition-colors hover:bg-hover disabled:opacity-50"
              >
                {starting ? <Spinner size={14} /> : t.detail.runDiarize}
              </button>
            </div>
          )}

          {/* AI議事録 */}
          {detail.summaries.length > 0 ? (
            <div className="mb-4 flex flex-col gap-3">
              {detail.summaries.map((s, i) => (
                <div
                  key={i}
                  className="rounded-card border border-border-2 bg-surface-2 px-[17px] py-[15px]"
                  style={{ borderLeft: "3px solid #6366F1" }}
                >
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <span className="flex min-w-0 items-center gap-2">
                      <span className="truncate text-[12px] font-bold tracking-[0.03em] text-brand-light">
                        {t.detail.summaryCardLabel(templateLabel(s.template_id))}
                      </span>
                      {/* 後付け話者分離などで古くなった要約に注意（ADR-0024）。 */}
                      {s.stale && (
                        <span
                          title={t.detail.summaryStaleTitle}
                          className="shrink-0 rounded-full bg-[rgba(245,158,11,0.15)] px-2 py-0.5 text-[10px] font-medium text-amber"
                        >
                          {t.detail.summaryStale}
                        </span>
                      )}
                    </span>
                    <button
                      onClick={() => openModal(s.template_id)}
                      className="inline-flex shrink-0 items-center gap-1 text-[11px] text-dim transition-colors hover:text-sub"
                      title={t.detail.regenerate}
                    >
                      <RefreshIcon size={13} />
                      {t.detail.regenerate}
                    </button>
                  </div>
                  <div className="whitespace-pre-wrap text-[13px] leading-[1.85] text-body">
                    {s.content}
                  </div>
                  {s.action_items.length > 0 && (
                    <ul className="mt-3 flex flex-col gap-1.5">
                      {s.action_items.map((a, j) => (
                        <li key={j} className="flex gap-2 text-[12.5px] text-body">
                          <CheckIcon size={14} className="mt-0.5 shrink-0 text-green" />
                          <span>
                            {a.text}
                            {a.assignee ? ` — ${a.assignee}` : ""}
                            {a.due ? `（${a.due}）` : ""}
                          </span>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <button
              onClick={() => openModal("minutes")}
              className="mb-4 flex w-full items-center gap-3 rounded-card border border-dashed border-border-3 bg-surface-2 px-4 py-4 text-left transition-colors hover:bg-hover"
            >
              <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[9px] bg-[rgba(99,102,241,0.15)] text-brand-lighter">
                <SparklesIcon size={17} />
              </span>
              <span className="min-w-0">
                <span className="block text-[13.5px] font-semibold text-ink">
                  {t.detail.createMinutesCta}
                </span>
                <span className="mt-0.5 block text-[12px] text-muted">
                  {t.detail.createMinutesCtaDesc}
                </span>
              </span>
            </button>
          )}

          {/* シリーズ横断ダイジェスト（モック画面へ）。配布時は隠す。 */}
          {MOCK_PREVIEW && (
            <button
              onClick={() => navigate({ view: "digest" })}
              className="mb-4 inline-flex items-center gap-1 text-[12px] text-brand-light transition-colors hover:text-brand-lighter"
            >
              シリーズの横断ダイジェスト
              <ArrowUpRightIcon size={13} />
            </button>
          )}

          {/* タブ */}
          <div className="mb-3 flex items-center border-b border-line">
            <TabButton active={tab === "transcript"} onClick={() => setTab("transcript")}>
              {t.detail.tabs.transcript}
            </TabButton>
            {/* チャプター/翻訳はモック（未実装）。配布時(MOCK_PREVIEW=false)は丸ごと隠し、
                実録音に固定ダミーが出ないようにする。 */}
            {MOCK_PREVIEW && (
              <TabButton active={tab === "chapters"} onClick={() => setTab("chapters")}>
                チャプター
              </TabButton>
            )}
            {tab === "transcript" && MOCK_PREVIEW && (
              <div className="ml-auto flex items-center gap-2 pb-1.5">
                <PreviewTag />
                <span className="text-[11.5px] text-muted">日本語に翻訳</span>
                <Toggle
                  checked={translateOn}
                  onChange={setTranslateOn}
                  label="日本語に翻訳"
                />
              </div>
            )}
          </div>

          {tab === "chapters" && MOCK_PREVIEW ? (
            <div>
              <div className="mb-3 flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="text-[13px] text-sub">{CHAPTERS.length} チャプター</span>
                  <PreviewTag />
                </div>
                <span className="inline-flex items-center gap-1.5 rounded-[7px] bg-[rgba(99,102,241,0.14)] px-2.5 py-1 text-[11px] text-brand-lighter">
                  <SparklesIcon size={12} />
                  AIが自動生成
                </span>
              </div>
              {/* タイムラインバー */}
              <div className="flex h-[9px] gap-[3px] overflow-hidden rounded-[5px]">
                {CHAPTERS.map((c) => (
                  <div
                    key={c.time}
                    className="rounded-[4px]"
                    style={{ flexGrow: c.grow, background: c.color }}
                  />
                ))}
              </div>
              <div className="mb-5 mt-1.5 flex justify-between font-mono text-[10px] text-dim">
                <span>00:00</span>
                <span>{formatDuration(rec.duration_ms)}</span>
              </div>
              <div className="flex flex-col gap-2.5">
                {CHAPTERS.map((c) => (
                  <div
                    key={c.time}
                    className="flex gap-3.5 rounded-card border border-border bg-surface px-4 py-3.5"
                  >
                    <div className="shrink-0 text-center">
                      <span className="font-mono text-[12px]" style={{ color: c.color }}>
                        {c.time}
                      </span>
                      <div
                        className="mx-auto mt-1.5 h-[9px] w-[9px] rounded-full"
                        style={{ background: c.color }}
                      />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-[13.5px] font-bold text-ink">{c.title}</span>
                        <span className="shrink-0 text-[10.5px] text-dim">{c.dur}</span>
                      </div>
                      <div className="mt-1.5 text-[12px] leading-relaxed text-muted">{c.body}</div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : detail.transcript.segments.length > 0 ? (
            <TranscriptList
              segments={detail.transcript.segments}
              speakers={detail.speakers}
              translate={
                translateOn && MOCK_PREVIEW ? () => MOCK_TRANSLATION : undefined
              }
              // 話者が 1 人も居ない録音（話者分離していない）では訂正の選択肢が無いので出さない。
              onSpeakerClick={
                detail.speakers && detail.speakers.length > 0 ? setFixingSeg : undefined
              }
            />
          ) : (
            <EmptyState
              title={t.detail.noTranscriptTitle}
              hint={t.detail.noTranscriptHint}
            />
          )}
        </div>
      </div>

      {/* 右ペイン */}
      <aside className="flex w-[222px] shrink-0 flex-col gap-4 overflow-y-auto border-l border-border bg-surface px-[15px] py-4">
        {speakers.length > 0 && (
          <SpeakerPanel speakers={speakers} recordingId={id} onRenamed={onRenamed} />
        )}

        {/* AIで作成（常設）。各アクションはそのテンプレへ preset してモーダルを開く。 */}
        <div>
          <div className="mb-2.5 text-[11px] font-bold tracking-[0.08em] text-dim">
            {t.detail.aiCreate}
          </div>
          <div className="flex flex-col gap-1.5">
            <button
              onClick={() => openModal("minutes")}
              className="h-9 w-full rounded-[8px] bg-brand text-[12.5px] font-semibold text-white transition-[filter] hover:brightness-110"
            >
              {t.detail.createMinutes}
            </button>
            <button
              onClick={() => openModal("summary")}
              className="h-[34px] w-full rounded-[8px] border border-border-2 text-[12px] text-body transition-colors hover:bg-hover"
            >
              {t.detail.createSummary}
            </button>
            <button
              onClick={() => openModal("action_items")}
              className="h-[34px] w-full rounded-[8px] border border-border-2 text-[12px] text-body transition-colors hover:bg-hover"
            >
              {t.detail.createActionItems}
            </button>
          </div>
        </div>

        <div className="mt-auto rounded-card border border-border bg-surface-2 px-3 py-2.5">
          <div className="flex items-start gap-2">
            <BotIcon size={14} className="mt-px shrink-0 text-brand-light" />
            <span className="text-[11px] leading-relaxed text-sub">{t.detail.mcpNote}</span>
          </div>
        </div>
      </aside>

      <TemplateModal
        key={`tpl-${id}`}
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        recordingId={id}
        transcript={detail.transcript}
        onCreated={onCreated}
        presetTemplate={presetTemplate}
      />
      <AskDrawer key={`ask-${id}`} open={askOpen} onClose={() => setAskOpen(false)} title={title} />
      <ConfirmDialog
        open={confirmDel}
        title={t.history.deleteConfirmTitle}
        body={t.history.deleteConfirmBody(title)}
        busy={deleting}
        onConfirm={onDelete}
        onCancel={() => setConfirmDel(false)}
      />

      {/* 発言単位の話者訂正（Issue #19）。押した発言だけを別の話者へ移す。 */}
      <Modal open={!!fixingSeg} onClose={() => setFixingSeg(null)} width={380}>
        {fixingSeg && (
          <>
            <ModalHeader
              title={t.composite.fixSpeakerHeading}
              onClose={() => setFixingSeg(null)}
            />
            <div className="px-5 py-4">
              {/* どの発言を直そうとしているかを示す（押し間違いに気づけるように）。 */}
              <p className="mb-3 rounded-[10px] bg-surface-2 px-3 py-2 text-[12.5px] leading-6 text-sub">
                <span className="mr-2 font-mono text-[11px] text-dim tnum">
                  {formatTimestamp(fixingSeg.start_ms)}
                </span>
                {fixingSeg.text}
              </p>
              <div className="flex flex-col gap-0.5">
                {detail.speakers?.map((sp) => (
                  <MenuItem
                    key={sp.id}
                    onClick={() => void fixSegmentSpeaker(fixingSeg.idx, sp.id)}
                    icon={
                      <span
                        className="inline-block h-2.5 w-2.5 rounded-full"
                        style={{ background: speakerChipStyle(sp.id).color }}
                      />
                    }
                    hint={sp.id === fixingSeg.speaker_id ? "✓" : undefined}
                  >
                    {speakerName(sp.id, detail.speakers, lang)}
                  </MenuItem>
                ))}
                <MenuItem
                  onClick={() => void fixSegmentSpeaker(fixingSeg.idx, null)}
                  hint={fixingSeg.speaker_id === null ? "✓" : undefined}
                >
                  {t.composite.fixSpeakerToUnknown}
                </MenuItem>
              </div>
            </div>
          </>
        )}
      </Modal>
    </div>
  );
}
