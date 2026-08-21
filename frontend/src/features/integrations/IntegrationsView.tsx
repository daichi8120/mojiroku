// 連携（連携はすべてこのページに集約）。
// ⚠️ カレンダー（取り込み）と 書き出し先 Notion/Slack は**実機能**。
//    会議プラットフォームのみ**プレビュー（モック）**で PreviewTag を出す。
// 外向きだが「取り込みはこの画面の表示時にこちらから GET するだけ・書き出しはボタンを押したときだけ」を明示する。
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useApp } from "@/lib/app";
import { translateError, useI18n } from "@/i18n";
import { cx } from "@/lib/cx";
import { INTEGRATIONS, MOCK_PREVIEW } from "@/lib/mockData";
import { useSettingsPatch } from "@/lib/useSettingsPatch";
import {
  deleteSecret,
  getSettings,
  hasSecret,
  listCalendarEvents,
  notionAccessiblePages,
  oauthConnect,
  setSettings,
  startMicRecording,
} from "@/lib/tauri";
import {
  CALENDAR_ICAL_KEY,
  GOOGLE_OAUTH_ACCESS_KEY,
  GOOGLE_OAUTH_REFRESH_KEY,
  GOOGLE_TOKEN_EXPIRY_KEY,
  NOTION_TOKEN_KEY,
  SLACK_WEBHOOK_KEY,
  formatEventTime,
  type CalendarEvent,
  type NotionPage,
  type Settings,
} from "@/lib/types";
import { Button, Spinner, StatusBadge, Toggle } from "@/components/ui";
import { PreviewTag } from "@/components/composite";
import {
  CalendarIcon,
  PlugIcon,
  RefreshIcon,
  ShieldIcon,
  TrashIcon,
  VideoIcon,
} from "@/components/icons";

/** Notion / Slack の「連携済み→再連携/解除 ／ 未連携→連携」ボタン行（両カードで DOM 同一）。 */
function ConnectRow({
  saved,
  busy,
  onConnect,
  onDisconnect,
  connectLabel,
}: {
  saved: boolean;
  busy: boolean;
  onConnect: () => void;
  onDisconnect: () => void;
  connectLabel: string;
}) {
  const { t } = useI18n();
  return (
    <div className="mt-3 flex items-center gap-2">
      {saved ? (
        <>
          <span className="flex items-center gap-1.5 text-[12px] text-green">
            <span className="inline-block h-1.5 w-1.5 rounded-full bg-green" />
            {t.integrations.connect.connected}
          </span>
          <Button size="sm" variant="secondary" onClick={onConnect} disabled={busy}>
            {t.integrations.connect.reconnect}
          </Button>
          <Button size="sm" variant="secondary" onClick={onDisconnect} disabled={busy}>
            {t.integrations.connect.disconnect}
          </Button>
        </>
      ) : (
        <Button size="sm" variant="primary" onClick={onConnect} disabled={busy}>
          {busy ? t.integrations.connect.waitingBrowser : connectLabel}
        </Button>
      )}
    </div>
  );
}

function SectionTitle({ children }: { children: ReactNode }) {
  return <h2 className="text-[15px] font-bold text-ink">{children}</h2>;
}

export function IntegrationsView() {
  const { toast, navigate } = useApp();
  const { t, lang } = useI18n();
  const preview = () => toast(t.integrations.previewToast, "info");

  // ── カレンダー（実機能）─────────────────────────────────────────
  // connected: null=確認中 / true=接続済み / false=未接続。URL 自体は JS に読み戻さない。
  const [connected, setConnected] = useState<boolean | null>(null);
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // 「記録を準備」二重起動の防止（await 中の連打で録音が二重に始まらないように）。
  const prepRef = useRef(false);

  // ── 書き出し先（実機能）─────────────────────────────────────────
  // 永続設定（settings.json）。Notion の書き出し先ページ（notion_parent_id）の保存に使う。
  const [cfg, setCfg] = useState<Settings | null>(null);
  // Notion 連携状態（OAuth トークンをキーチェーンに保存済みか）と、書き出し先候補ページ、処理中フラグ。
  const [notionSaved, setNotionSaved] = useState(false);
  const [notionPages, setNotionPages] = useState<NotionPage[]>([]);
  const [notionBusy, setNotionBusy] = useState(false);
  // Slack 連携状態（OAuth で得た Webhook URL をキーチェーンに保存済みか）と処理中フラグ。
  const [slackSaved, setSlackSaved] = useState(false);
  const [slackBusy, setSlackBusy] = useState(false);

  // 設定の部分更新 + 永続化（read-modify-write）。SettingsView / 言語切替と同じ
  // settings.json を触るため、古いスナップショットでの巻き戻しを防ぐよう useSettingsPatch に
  // 集約している（実装はそちら）。
  const patch = useSettingsPatch(cfg, setCfg);

  // マウント時: 設定を読み、Notion/Slack の連携状態を確認する。
  // Notion 連携済みなら書き出し先候補を取得し、候補が 1 件だけなら自動選択して「ワンクリック」に近づける。
  useEffect(() => {
    let active = true;
    (async () => {
      try {
        const s = await getSettings();
        const notionHas = await hasSecret(NOTION_TOKEN_KEY);
        const slackHas = await hasSecret(SLACK_WEBHOOK_KEY);
        if (!active) return;
        setCfg(s);
        setNotionSaved(notionHas);
        setSlackSaved(slackHas);
        if (notionHas) {
          try {
            const pages = await notionAccessiblePages();
            if (!active) return;
            setNotionPages(pages);
            if (!s.notion_parent_id.trim() && pages.length === 1) {
              setCfg({ ...s, notion_parent_id: pages[0].id });
              // 読み直してから差し替え（言語設定など他画面の変更を巻き戻さない）。
              void getSettings()
                .then((cur) => setSettings({ ...cur, notion_parent_id: pages[0].id }))
                .catch(() => {});
            }
          } catch {
            /* ページ取得失敗は致命的でない（連携自体は保たれる） */
          }
        }
      } catch (e) {
        if (active) toast(t.settings.loadFailed(translateError(e, t)), "error");
      }
    })();
    return () => {
      active = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Notion と OAuth 連携（mojiroku.com の Worker ブローカー経由）。ブラウザで同意 → アクセストークンが
  // キーチェーンへ保存される。完了後、共有を許可したページを取得し、書き出し先候補として提示する。
  const connectNotion = async () => {
    if (notionBusy) return;
    setNotionBusy(true);
    try {
      await oauthConnect("notion");
      setNotionSaved(true);
      const pages = await notionAccessiblePages();
      setNotionPages(pages);
      if (pages.length === 1 && cfg && !cfg.notion_parent_id.trim()) {
        patch({ notion_parent_id: pages[0].id });
      }
      if (pages.length === 0) {
        toast(t.integrations.notion.noPagesToast, "info");
      } else {
        toast(t.integrations.notion.connectedToast, "success");
      }
    } catch (e) {
      toast(t.integrations.notion.connectFailed(translateError(e, t)), "error");
    } finally {
      setNotionBusy(false);
    }
  };

  // 連携解除（保存済みトークンを削除し、書き出し先も初期化）。
  const disconnectNotion = async () => {
    if (notionBusy) return;
    setNotionBusy(true);
    try {
      await deleteSecret(NOTION_TOKEN_KEY);
      setNotionSaved(false);
      setNotionPages([]);
      patch({ notion_parent_id: "" });
      toast(t.integrations.notion.disconnectedToast, "info");
    } catch (e) {
      toast(t.integrations.disconnectFailed(translateError(e, t)), "error");
    } finally {
      setNotionBusy(false);
    }
  };

  // Slack と OAuth 連携（loopback + PKCE）。ブラウザで同意 → Webhook URL がキーチェーンへ保存される。
  // 同意完了まで解決しないので busy 表示を出す。
  const connectSlack = async () => {
    if (slackBusy) return;
    setSlackBusy(true);
    try {
      await oauthConnect("slack");
      setSlackSaved(true);
      toast(t.integrations.slack.connectedToast, "success");
    } catch (e) {
      toast(t.integrations.slack.connectFailed(translateError(e, t)), "error");
    } finally {
      setSlackBusy(false);
    }
  };

  // 連携解除（保存済み Webhook URL を削除）。
  const disconnectSlack = async () => {
    if (slackBusy) return;
    setSlackBusy(true);
    try {
      await deleteSecret(SLACK_WEBHOOK_KEY);
      setSlackSaved(false);
      toast(t.integrations.slack.disconnectedToast, "info");
    } catch (e) {
      toast(t.integrations.disconnectFailed(translateError(e, t)), "error");
    } finally {
      setSlackBusy(false);
    }
  };

  const loadEvents = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setEvents(await listCalendarEvents());
    } catch (e) {
      // 原文（キー）のまま保持し、表示時に translateError する（言語切替にも追従させる）。
      setError(String(e));
      setEvents([]);
    } finally {
      setLoading(false);
    }
  }, []);

  // 初回: 接続済みか確認し、接続済みなら予定を取得。
  useEffect(() => {
    let active = true;
    (async () => {
      try {
        // OAuth（新方式）または iCal（旧方式・既存ユーザー）のいずれかで連携済みとみなす。
        const [googleHas, icalHas] = await Promise.all([
          hasSecret(GOOGLE_OAUTH_REFRESH_KEY),
          hasSecret(CALENDAR_ICAL_KEY),
        ]);
        if (!active) return;
        const has = googleHas || icalHas;
        setConnected(has);
        if (has) void loadEvents();
      } catch {
        if (active) setConnected(false);
      }
    })();
    return () => {
      active = false;
    };
  }, [loadEvents]);

  // Google と OAuth 連携（loopback + PKCE）。ブラウザで同意するとトークンがキーチェーンへ保存される。
  // 同意完了まで解決しないので busy 表示を出す。
  const connect = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await oauthConnect("google");
      setConnected(true);
      toast(t.integrations.calendar.connectedToast, "success");
      await loadEvents();
    } catch (e) {
      toast(t.integrations.calendar.connectFailed(translateError(e, t)), "error");
    } finally {
      setBusy(false);
    }
  };

  // 連携解除（OAuth トークン・旧 iCal URL の両方を削除）。
  const disconnect = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await Promise.all([
        deleteSecret(GOOGLE_OAUTH_ACCESS_KEY),
        deleteSecret(GOOGLE_OAUTH_REFRESH_KEY),
        deleteSecret(GOOGLE_TOKEN_EXPIRY_KEY),
        deleteSecret(CALENDAR_ICAL_KEY),
      ]);
      setConnected(false);
      setEvents([]);
      setError(null);
      toast(t.integrations.calendar.disconnectedToast, "info");
    } catch (e) {
      toast(t.integrations.disconnectFailed(translateError(e, t)), "error");
    } finally {
      setBusy(false);
    }
  };

  // 予定タイトルで録音を開始（HomeView の startMic と同等 + タイトル付与）。
  // カレンダーの予定は定義上「会議＝複数話者」なので話者分離を既定 ON にする（予定取得時点で
  // オンラインのためモデル DL の懸念も小さい）。
  const prepare = async (ev: CalendarEvent) => {
    if (prepRef.current) return;
    prepRef.current = true;
    try {
      await startMicRecording();
      navigate({ view: "recording", diarize: true, title: ev.title });
    } catch (e) {
      toast(translateError(e, t), "error");
    } finally {
      prepRef.current = false;
    }
  };

  return (
    <div className="mx-auto flex max-w-[720px] flex-col gap-7 px-8 py-10">
      <header>
        <div className="flex items-center gap-2.5">
          <span className="flex h-9 w-9 items-center justify-center rounded-btn bg-[rgba(167,139,250,0.14)] text-purple">
            <PlugIcon size={18} />
          </span>
          <h1 className="text-[17px] font-bold text-ink">{t.integrations.title}</h1>
        </div>
        <p className="mt-1.5 text-[13px] text-muted">{t.integrations.intro}</p>
      </header>

      {/* ── カレンダー（実機能）─────────────────────────── */}
      <section className="flex flex-col gap-3.5">
        <div>
          <SectionTitle>
            <span className="inline-flex items-center gap-2">
              <CalendarIcon size={16} className="text-purple" />
              {t.integrations.calendar.title}
            </span>
          </SectionTitle>
          <p className="mt-1 text-[12px] text-muted">{t.integrations.calendar.desc}</p>
        </div>

        {connected === null ? (
          <div className="flex items-center gap-2 rounded-card border border-border bg-surface px-4 py-3.5 text-[12px] text-muted">
            <Spinner size={15} /> {t.integrations.calendar.checking}
          </div>
        ) : connected ? (
          <>
            {/* 接続済みカード（⚠️ URL/メールは表示しない。状態のみ） */}
            <div className="rounded-card border border-border bg-surface px-4 py-3.5">
              <div className="flex items-center gap-3">
                <span className="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-[9px] bg-[rgba(99,102,241,0.15)] text-brand-light">
                  <CalendarIcon size={17} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="text-[13px] font-semibold text-ink">
                    {t.integrations.calendar.googleName}
                  </div>
                  <div className="mt-0.5 text-[11px] text-green">
                    {t.integrations.calendar.connectedBadge}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="secondary"
                  icon={<RefreshIcon size={13} />}
                  onClick={loadEvents}
                  disabled={loading || busy}
                >
                  {t.integrations.calendar.refresh}
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  icon={<TrashIcon size={13} />}
                  onClick={disconnect}
                  disabled={busy}
                >
                  {t.integrations.calendar.disconnect}
                </Button>
              </div>
            </div>

            {/* 次の予定 */}
            <div className="overflow-hidden rounded-card border border-border bg-surface">
              <div className="border-b border-line px-4 py-2.5 text-[11px] text-dim">
                {t.integrations.calendar.upcoming}
              </div>
              {loading ? (
                <div className="flex items-center gap-2 px-4 py-5 text-[12px] text-muted">
                  <Spinner size={15} /> {t.integrations.calendar.loadingEvents}
                </div>
              ) : error ? (
                <div className="px-4 py-4 text-[12px] text-red-light">
                  {t.integrations.calendar.loadFailed(translateError(error, t))}
                </div>
              ) : events.length === 0 ? (
                <div className="px-4 py-5 text-[12px] text-muted">
                  {t.integrations.calendar.noEvents}
                </div>
              ) : (
                events.map((ev, i) => (
                  <div
                    key={ev.id}
                    className={cx(
                      "flex items-center gap-3 px-4 py-3",
                      i < events.length - 1 && "border-b border-line",
                    )}
                  >
                    <div className="w-[72px] shrink-0 font-mono text-[12px] text-body tnum">
                      {formatEventTime(ev.start, lang)}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13px] text-ink">{ev.title}</div>
                      {ev.location && (
                        <div className="mt-0.5 truncate text-[10.5px] text-faint">{ev.location}</div>
                      )}
                    </div>
                    <Button size="sm" variant="primary" onClick={() => prepare(ev)}>
                      {t.integrations.calendar.prepare}
                    </Button>
                  </div>
                ))
              )}
            </div>
            <p className="text-[10.5px] text-faint">{t.integrations.calendar.cacheNote}</p>
          </>
        ) : (
          // 未連携: Google OAuth ワンクリック連携
          <div className="rounded-card border border-border bg-surface p-4">
            <div className="text-[13px] font-semibold text-ink">
              {t.integrations.calendar.connectTitle}
            </div>
            <div className="mt-1 text-[11.5px] text-sub">{t.integrations.calendar.connectDesc}</div>
            <div className="mt-3">
              <Button variant="primary" onClick={connect} disabled={busy}>
                {busy ? t.integrations.connect.waitingBrowser : t.integrations.calendar.connectCta}
              </Button>
            </div>

            <div className="mt-3 rounded-btn border border-border-2 bg-surface-2 p-3 text-[11px] leading-relaxed text-muted">
              <div className="font-semibold text-sub">{t.integrations.howToTitle}</div>
              <ol className="mt-1 list-decimal space-y-0.5 pl-4">
                <li>{t.integrations.calendar.step1}</li>
                <li>
                  {t.integrations.calendar.step2}
                  <span className="text-faint">{t.integrations.calendar.step2Note}</span>
                </li>
                <li>{t.integrations.calendar.step3}</li>
              </ol>
            </div>
          </div>
        )}
      </section>

      {/* ── 会議プラットフォーム（プレビュー。MOCK_PREVIEW=false でセクションごと消える）── */}
      {MOCK_PREVIEW && (
      <section className="flex flex-col gap-3.5">
        <div>
          <SectionTitle>
            <span className="inline-flex items-center gap-2">
              {t.integrations.platforms.title}
              <PreviewTag />
            </span>
          </SectionTitle>
          <p className="mt-1 text-[12px] text-muted">
            {t.integrations.platforms.desc}
            <span className="font-medium text-green-light"> {t.integrations.platforms.noBots}</span>
          </p>
        </div>

        <div className="overflow-hidden rounded-card border border-border bg-surface">
          {INTEGRATIONS.platforms.map((p, i) => (
            <div
              key={p.id}
              className={cx(
                "flex items-center gap-3 px-4 py-3",
                i < INTEGRATIONS.platforms.length - 1 && "border-b border-line",
              )}
            >
              <span
                className={cx(
                  "flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-btn bg-surface-2",
                  p.connected ? "text-brand-light" : "text-muted",
                )}
              >
                <VideoIcon size={15} />
              </span>
              <div className="min-w-0 flex-1 text-[13px] text-ink">{p.name}</div>
              <Toggle
                checked={p.connected}
                onChange={preview}
                label={t.integrations.platforms.toggleLabel(p.name)}
              />
            </div>
          ))}
        </div>
      </section>
      )}

      {/* ── 書き出し先（実機能・Notion / Slack）──────────────── */}
      <section className="flex flex-col gap-3.5">
        <div>
          <SectionTitle>{t.integrations.export.title}</SectionTitle>
          <p className="mt-1 text-[12px] text-muted">{t.integrations.export.desc}</p>
        </div>

        {/* Notion */}
        <div className="rounded-card border border-border bg-surface p-4">
          <div className="flex items-center gap-2">
            <div className="text-[13.5px] font-bold text-ink">Notion</div>
            {notionSaved && cfg?.notion_parent_id.trim() ? (
              <StatusBadge tone="green">{t.integrations.export.configured}</StatusBadge>
            ) : notionSaved ? (
              <StatusBadge tone="amber">{t.integrations.export.pageNotSelected}</StatusBadge>
            ) : (
              <StatusBadge tone="neutral">{t.integrations.export.notConfigured}</StatusBadge>
            )}
          </div>
          <div className="mt-1 text-[11.5px] text-sub">{t.integrations.notion.desc}</div>

          {/* 連携ボタン（OAuth・Worker ブローカー経由。事前のインテグレーション作成やトークン貼り付けは不要） */}
          <ConnectRow
            saved={notionSaved}
            busy={notionBusy}
            onConnect={connectNotion}
            onDisconnect={disconnectNotion}
            connectLabel={t.integrations.notion.connectCta}
          />

          {/* 書き出し先ページ（連携後に共有を許可したページから選ぶ） */}
          {notionSaved && (
            <div className="mt-3">
              <div className="mb-1.5 text-[11.5px] text-sub">{t.integrations.notion.parentLabel}</div>
              {notionPages.length > 0 ? (
                <select
                  value={cfg?.notion_parent_id ?? ""}
                  onChange={(e) => patch({ notion_parent_id: e.target.value })}
                  className="w-full rounded-btn border border-border-2 bg-surface-2 px-3 py-2.5 text-[12.5px] text-body focus:border-brand focus:outline-none"
                >
                  <option value="">{t.integrations.notion.parentPlaceholder}</option>
                  {notionPages.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.title}
                    </option>
                  ))}
                </select>
              ) : (
                <div className="rounded-btn border border-border-2 bg-surface-2 px-3 py-2.5 text-[11.5px] text-muted">
                  {t.integrations.notion.noPages}
                </div>
              )}
            </div>
          )}

          {/* はじめ方（手順ヘルプ） */}
          <div className="mt-3 rounded-btn border border-border-2 bg-surface-2 p-3 text-[11px] leading-relaxed text-muted">
            <div className="font-semibold text-sub">{t.integrations.howToTitle}</div>
            <ol className="mt-1 list-decimal space-y-0.5 pl-4">
              <li>{t.integrations.notion.step1}</li>
              <li>{t.integrations.notion.step2}</li>
              <li>{t.integrations.notion.step3}</li>
            </ol>
          </div>

          {/* 開示（要約エンジンに依存せず常に表示） */}
          <div className="mt-2.5 flex items-start gap-1.5">
            <ShieldIcon size={12} className="mt-px shrink-0 text-amber" />
            <span className="text-[11px] text-amber">
              {t.integrations.notion.notePre}
              <strong className="font-semibold">{t.integrations.notion.noteStrong}</strong>
              {t.integrations.notion.notePost}
            </span>
          </div>
        </div>

        {/* Slack */}
        <div className="rounded-card border border-border bg-surface p-4">
          <div className="flex items-center gap-2">
            <div className="text-[13.5px] font-bold text-ink">Slack</div>
            {slackSaved ? (
              <StatusBadge tone="green">{t.integrations.export.configured}</StatusBadge>
            ) : (
              <StatusBadge tone="neutral">{t.integrations.export.notConfigured}</StatusBadge>
            )}
          </div>
          <div className="mt-1 text-[11.5px] text-sub">{t.integrations.slack.desc}</div>

          {/* 連携ボタン（OAuth・loopback + PKCE。事前のアプリ作成や URL 貼り付けは不要） */}
          <ConnectRow
            saved={slackSaved}
            busy={slackBusy}
            onConnect={connectSlack}
            onDisconnect={disconnectSlack}
            connectLabel={t.integrations.slack.connectCta}
          />

          {/* はじめ方（手順ヘルプ） */}
          <div className="mt-3 rounded-btn border border-border-2 bg-surface-2 p-3 text-[11px] leading-relaxed text-muted">
            <div className="font-semibold text-sub">{t.integrations.howToTitle}</div>
            <ol className="mt-1 list-decimal space-y-0.5 pl-4">
              <li>{t.integrations.slack.step1}</li>
              <li>{t.integrations.slack.step2}</li>
              <li>{t.integrations.slack.step3}</li>
            </ol>
          </div>

          {/* 開示（要約エンジンに依存せず常に表示） */}
          <div className="mt-2.5 flex items-start gap-1.5">
            <ShieldIcon size={12} className="mt-px shrink-0 text-amber" />
            <span className="text-[11px] text-amber">
              {t.integrations.slack.notePre}
              <strong className="font-semibold">{t.integrations.slack.noteStrong}</strong>
              {t.integrations.slack.notePost}
            </span>
          </div>
        </div>
      </section>

      <p className="text-[11.5px] text-faint">{t.integrations.footer}</p>
    </div>
  );
}
