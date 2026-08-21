// 左サイドバー（236px 固定）。ナビ + 最近リスト + ローカルステータス。
import { useApp, type ViewKind } from "@/lib/app";
import { useI18n } from "@/i18n";
import { cx } from "@/lib/cx";
import { openFeedbackForm } from "@/lib/feedback";
import { formatDurationHuman, type Recording } from "@/lib/types";
import { LocalStatus } from "./composite";
import {
  BrandMark,
  CalendarIcon,
  HomeIcon,
  MessageIcon,
  PlugIcon,
  PlusIcon,
  SettingsIcon,
  UsersIcon,
  VideoIcon,
} from "./icons";

// ナビの view はモジュールスコープで持ち、表示ラベルは描画時に t.sidebar.nav[view] で引く。
type NavView = Extract<
  ViewKind,
  "meeting" | "home" | "history" | "speakers" | "integrations" | "settings"
>;

interface NavItem {
  view: NavView;
  icon: (p: { size?: number; className?: string }) => React.ReactNode;
}

// 全項目が実機能（モック画面はナビ直下には無く、到達性は MOCK_PREVIEW が握る）。
const NAV: NavItem[] = [
  { view: "meeting", icon: VideoIcon },
  { view: "home", icon: HomeIcon },
  { view: "history", icon: CalendarIcon },
  { view: "speakers", icon: UsersIcon },
  { view: "integrations", icon: PlugIcon },
  { view: "settings", icon: SettingsIcon },
];

export function Sidebar({
  recents,
  activeJobIds,
}: {
  recents: Recording[];
  /** 進行中ジョブ（pending|running）を持つ録音 id。最近リストに処理中ドットを出す（ADR-0024）。 */
  activeJobIds?: Set<string>;
}) {
  const { route, navigate, meeting } = useApp();
  const { t, lang } = useI18n();
  const active = route.view;
  // 会議録音中はナビ上に録音インジケータを出す（遷移しても録音は継続している）。
  const meetingRecording = meeting.status !== "idle";
  // 録音中は唯一の停止導線（RecordingView の停止ボタン）に収束させる。
  // ここで離脱できると録音がバックエンドに残り停止/保存できなくなる。
  const locked = route.view === "recording";

  return (
    <aside className="flex h-full w-[236px] shrink-0 flex-col border-r border-border bg-surface">
      {/* ロゴ */}
      <div className="flex items-center gap-2.5 px-4 pb-3 pt-4">
        <BrandMark size={28} className="rounded-[8px]" />
        <span className="text-[16px] font-bold tracking-tight text-ink">mojiroku</span>
      </div>

      {/* 新しい録音 */}
      <div className="px-3 pb-2">
        <button
          onClick={() => navigate({ view: "home" })}
          disabled={locked}
          className="flex w-full items-center justify-center gap-2 rounded-btn py-2.5 text-[13px] font-medium text-white transition-[filter] hover:brightness-110 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:brightness-100"
          style={{ background: "linear-gradient(180deg,#6366F1,#4F46E5)" }}
        >
          <PlusIcon size={16} />
          {t.sidebar.newRecording}
        </button>
      </div>

      {/* ナビ */}
      <nav className="flex flex-col gap-0.5 px-3 py-2">
        {NAV.map((item) => {
          const Icon = item.icon;
          const isActive = active === item.view;
          return (
            <button
              key={item.view}
              onClick={() => navigate({ view: item.view })}
              disabled={locked}
              className={cx(
                "flex items-center gap-2.5 rounded-[9px] px-2.5 py-2 text-[13px] transition-colors",
                isActive
                  ? "bg-hover text-ink"
                  : "text-sub hover:bg-hover/60 hover:text-body",
                locked && "opacity-40 pointer-events-none",
              )}
            >
              <Icon size={17} className={isActive ? "text-brand-light" : undefined} />
              <span className="flex-1 text-left">{t.sidebar.nav[item.view]}</span>
              {item.view === "meeting" && meetingRecording && (
                <span
                  className="h-2 w-2 animate-mjpulse rounded-full bg-red shadow-[0_0_0_3px_rgba(239,68,68,0.18)]"
                  title={t.sidebar.recordingDot}
                />
              )}
            </button>
          );
        })}
      </nav>

      {/* 最近 */}
      <div className="mt-1 flex min-h-0 flex-1 flex-col px-3">
        <div className="px-2.5 pb-1.5 pt-2 text-[10.5px] font-medium uppercase tracking-[0.08em] text-dim">
          {t.sidebar.recent}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {recents.length === 0 ? (
            <div className="px-2.5 py-2 text-[12px] text-dim">{t.sidebar.recentEmpty}</div>
          ) : (
            recents.slice(0, 12).map((r) => (
              <button
                key={r.id}
                onClick={() => navigate({ view: "detail", id: r.id })}
                disabled={locked}
                className={cx(
                  "flex w-full flex-col rounded-[8px] px-2.5 py-1.5 text-left transition-colors hover:bg-hover/60",
                  route.view === "detail" && route.id === r.id && "bg-hover",
                  locked && "opacity-40 pointer-events-none",
                )}
              >
                <span className="flex items-center gap-1.5">
                  <span className="truncate text-[12.5px] text-body">
                    {r.title || t.common.untitled}
                  </span>
                  {activeJobIds?.has(r.id) && (
                    <span
                      className="h-1.5 w-1.5 shrink-0 animate-mjpulse rounded-full bg-brand-light"
                      title={t.job.processing}
                    />
                  )}
                </span>
                <span className="font-mono text-[10.5px] text-dim tnum">
                  {formatDurationHuman(r.duration_ms, lang)}
                </span>
              </button>
            ))
          )}
        </div>
      </div>

      {/* フィードバック（ベータの収集導線。外部ブラウザで開くだけなので録音中も無効化しない） */}
      <div className="px-3 pt-2">
        <button
          onClick={() => void openFeedbackForm().catch(() => {})}
          className="flex w-full items-center gap-2.5 rounded-[9px] px-2.5 py-2 text-[12.5px] text-sub transition-colors hover:bg-hover/60 hover:text-body"
        >
          <MessageIcon size={16} />
          <span className="flex-1 text-left">{t.sidebar.sendFeedback}</span>
        </button>
      </div>

      {/* ステータス */}
      <div className="p-3">
        <LocalStatus />
      </div>
    </aside>
  );
}
