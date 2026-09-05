// 設定（Studio 07）。モデル管理 / 要約エンジン / プライバシー / 一般。
// 設定は settings.json（app_data_dir）に永続化し、BYOK API キーは OS キーチェーンに保管する。
// マウント時に load、変更で即 save（getSettings/setSettings + set_secret/has_secret/delete_secret）。
// engine/provider/model は要約コマンドに実効。プライバシー項目は値の保存のみ（挙動反映は近日）。
// ※ モデルの「管理/取得」ボタンの実処理（DL 管理）は未配線で toast で予告する。
import { useEffect, useRef, useState, type ReactNode } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useApp } from "@/lib/app";
import { translateError, useI18n } from "@/i18n";
import { cx } from "@/lib/cx";
import { openFeedbackForm } from "@/lib/feedback";
import { byokKeyName, type Settings } from "@/lib/types";
import {
  deleteSecret,
  getSettings,
  hasSecret,
  setSecret,
  summaryModelInfo,
  transcriptionModelInfo,
  type SummaryModelInfo,
  type TranscriptionModelInfo,
} from "@/lib/tauri";
import { useSettingsPatch } from "@/lib/useSettingsPatch";
import { Button, StatusBadge, Toggle } from "@/components/ui";
import { BrandMark, ChevronDownIcon, LayersIcon, MessageIcon, MicIcon, ShieldIcon, UsersIcon } from "@/components/icons";

// プロバイダ別の既定モデル/キー形式（プレースホルダ表示用。実値は編集可能）。
// Rust 側 settings.rs の *_DEFAULT_MODEL と揃える（claude-3-5-sonnet は提供終了済み）。
const MODEL_PLACEHOLDER: Record<Settings["provider"], string> = {
  anthropic: "claude-sonnet-4-6",
  openai: "gpt-4o-mini",
};
const PROVIDER_LABEL: Record<Settings["provider"], string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
};
const KEY_PLACEHOLDER: Record<Settings["provider"], string> = {
  anthropic: "sk-ant-••••",
  openai: "sk-••••",
};

type SectionKey = "models" | "engine" | "privacy" | "general";

// ラベルは t.settings.nav[key] から引く（言語切替に追従させるため文字列は持たない）。
const NAV_KEYS: SectionKey[] = ["models", "engine", "privacy", "general"];

const SECTION_TITLE = "text-[15px] font-bold text-ink";
const SECTION_DESC = "mt-1 text-[12px] text-muted";

export function SettingsView() {
  const { toast } = useApp();
  const { t, lang, setLang } = useI18n();

  // アプリバージョン（一般セクションに表示）。取得失敗時は空のまま。
  const [appVersion, setAppVersion] = useState("");
  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  // 要約モデルは端末のメモリで変わる（ADR-0030）。**固定文字列で書かない。**
  // 取得に失敗したら行は出すが型名は空にする（嘘の型名を出すより無いほうがよい）。
  const [summaryModel, setSummaryModel] = useState<SummaryModelInfo | null>(null);
  const [transcriptionModel, setTranscriptionModel] = useState<TranscriptionModelInfo | null>(null);
  useEffect(() => {
    transcriptionModelInfo().then(setTranscriptionModel).catch(() => {});
  }, []);
  useEffect(() => {
    summaryModelInfo().then(setSummaryModel).catch(() => {});
  }, []);

  // ── サブナビのアクティブ表示 ──
  // クリックで該当セクションへスムーズスクロールし、アクティブ表示を更新する。
  // （手動スクロールには追従しない。スクロールスパイは未実装。）
  const [active, setActive] = useState<SectionKey>("models");
  const refs = {
    models: useRef<HTMLDivElement>(null),
    engine: useRef<HTMLDivElement>(null),
    privacy: useRef<HTMLDivElement>(null),
    general: useRef<HTMLDivElement>(null),
  };
  const goto = (key: SectionKey) => {
    setActive(key);
    refs[key].current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  // ── 永続設定（settings.json）。マウント時に load、変更で即 save ──
  const [cfg, setCfg] = useState<Settings | null>(null);

  // The summarize row shows the model that will actually run. With an explicit choice the
  // row is derived from `choices` on the client, so it updates the moment the user picks
  // without re-fetching (the patch saves asynchronously, so a re-fetch could read the old
  // settings). Without a choice, automatic (model on disk first, then tier) is shown; this
  // also covers switching back to automatic after a persisted explicit choice.
  // A stale file name that is no longer offered counts as automatic, matching core.
  const summaryChoice = summaryModel?.choices.find((c) => c.file === cfg?.local_summary_model);
  const shownSummaryModel = summaryChoice ?? summaryModel?.auto;
  const shownTranscriptionModel = transcriptionModel?.choices.find(
    (model) => model.file === cfg?.transcription_model.trim(),
  ) ?? transcriptionModel?.choices.find((model) => model.file === transcriptionModel.default_file);

  // 入力中の API キー（平文。保存後は state に残さない）と、キーチェーン保存済みフラグ。
  const [apiKey, setApiKey] = useState("");
  const [keySaved, setKeySaved] = useState(false);
  const [keyBusy, setKeyBusy] = useState(false);

  // マウント時のみ。設定を読み、BYOK キーの保存状態を確認する（toast は catch のみ）。
  useEffect(() => {
    let active = true;
    (async () => {
      try {
        const s = await getSettings();
        const has = await hasSecret(byokKeyName(s.provider));
        if (!active) return;
        setCfg(s);
        setKeySaved(has);
      } catch (e) {
        if (active) toast(t.settings.loadFailed(translateError(e, t)), "error");
      }
    })();
    return () => {
      active = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 設定の部分更新 + 永続化（同じ値の再保存は冪等。失敗は toast）。
  // 設定の部分更新 + 永続化（read-modify-write）。IntegrationsView / 言語切替（App.tsx）と
  // 同じ settings.json を触るため、古いスナップショットで他画面の変更を巻き戻さないよう
  // useSettingsPatch に集約している（実装はそちら）。
  const patch = useSettingsPatch(cfg, setCfg);

  // provider 切替: model をリセット（前 provider 用モデル名の誤送信を防ぐ）し、
  // 入力中キーを破棄、新 provider スロットの保存状態を読み直す。
  const changeProvider = async (next: Settings["provider"]) => {
    if (next === provider) return;
    patch({ provider: next, model: "" });
    setApiKey("");
    try {
      setKeySaved(await hasSecret(byokKeyName(next)));
    } catch {
      setKeySaved(false);
    }
  };

  // BYOK キーを provider 別スロットへ保存（保存後は入力欄をクリアし、平文を state に残さない）。
  const saveKey = async () => {
    const v = apiKey.trim();
    if (!v || keyBusy) return;
    setKeyBusy(true);
    try {
      await setSecret(byokKeyName(provider), v);
      setKeySaved(true);
      setApiKey("");
      toast(t.settings.engine.keySavedToast, "success");
    } catch (e) {
      toast(t.settings.engine.keySaveFailed(translateError(e, t)), "error");
    } finally {
      setKeyBusy(false);
    }
  };

  const clearKey = async () => {
    if (keyBusy) return;
    setKeyBusy(true);
    try {
      await deleteSecret(byokKeyName(provider));
      setKeySaved(false);
      setApiKey("");
      toast(t.settings.engine.keyDeletedToast, "info");
    } catch (e) {
      toast(t.settings.engine.keyDeleteFailed(translateError(e, t)), "error");
    } finally {
      setKeyBusy(false);
    }
  };

  const engine = cfg?.engine ?? "local";
  const provider = cfg?.provider ?? "anthropic";
  const notYet = () => toast(t.settings.models.manageSoon, "info");

  return (
    <div className="flex min-h-full">
      {/* 設定サブナビ 200px */}
      {/* h-full は sticky を無効化する（内容と同じ高さになり貼り付く余地が消える）。
          h-screen + self-start でビューポート高に固定し、スクロール中も追従させる。 */}
      <aside className="sticky top-0 h-screen w-[200px] shrink-0 self-start border-r border-line bg-surface px-3 py-4">
        <div className="flex items-center gap-2.5 px-1.5 pb-3.5">
          <BrandMark size={28} className="rounded-lg" />
          <div className="text-[15px] font-bold text-ink">{t.settings.title}</div>
        </div>
        {NAV_KEYS.map((key) => (
          <button
            key={key}
            onClick={() => goto(key)}
            className={cx(
              "mb-0.5 flex w-full items-center rounded-lg px-3 py-2.5 text-left text-[12.5px] transition-colors",
              active === key
                ? "bg-selected font-semibold text-ink"
                : "text-sub hover:bg-hover",
            )}
          >
            {t.settings.nav[key]}
          </button>
        ))}
      </aside>

      {/* セクション */}
      <div className="min-w-0 flex-1 px-8 py-6">
        <div className="mx-auto max-w-[640px]">
          {/* ── モデル ── */}
          <section ref={refs.models} className="scroll-mt-6">
            <div className={SECTION_TITLE}>{t.settings.nav.models}</div>
            <div className={SECTION_DESC}>{t.settings.models.desc}</div>
            <div className="mt-3.5 overflow-hidden rounded-card border border-border bg-surface">
              <ModelRow
                icon={<MicIcon size={17} />}
                tint="bg-brand/15 text-brand-light"
                name={t.settings.models.stt}
                model={shownTranscriptionModel?.label ?? ""}
                size={shownTranscriptionModel?.size ?? ""}
                status={shownTranscriptionModel?.downloaded ? "saved" : "ondemand"}
                action={t.settings.models.manage}
                onAction={notYet}
              />
              {transcriptionModel && shownTranscriptionModel && (
                <>
                  <SelectRow
                    stacked
                    title={t.settings.models.transcriptionPickerLabel}
                    desc={t.settings.models.transcriptionPickerDesc}
                    value={shownTranscriptionModel.file}
                    onChange={(value) => patch({ transcription_model: value })}
                    options={transcriptionModel.choices.map((model) => ({
                      value: model.file,
                      label: `${model.label} · ${model.size}${model.downloaded ? "" : ` · ${t.settings.models.needsDownload}`}`,
                    }))}
                  />
                  {!shownTranscriptionModel.downloaded && (
                    <p className="border-b border-line px-4 py-2.5 text-[11.5px] text-muted">
                      {t.settings.models.transcriptionWillDownload(shownTranscriptionModel.size)}
                    </p>
                  )}
                </>
              )}
              <ModelRow
                icon={<LayersIcon size={17} />}
                tint="bg-cyan/15 text-teal"
                name={t.settings.models.summarize}
                model={shownSummaryModel?.label ?? ""}
                size={shownSummaryModel?.size ?? ""}
                status={shownSummaryModel?.downloaded ? "saved" : "ondemand"}
                action={t.settings.models.manage}
                onAction={notYet}
              />
              <ModelRow
                icon={<UsersIcon size={17} />}
                tint="bg-amber/15 text-amber"
                name={t.settings.models.diarize}
                model="sherpa-onnx (pyannote)"
                size="110MB"
                status="ondemand"
                action={t.settings.models.fetch}
                onAction={notYet}
                last={!summaryModel}
              />
              {/* Explicit summary-model switch (ADR-0030). "" = automatic. Only adopted models
                  are offered; a choice above this Mac's tier stays selectable but is warned
                  about (Issue #30). The download happens at the next summary, through the
                  existing progress flow, never here. */}
              {summaryModel && (
                <>
                  <SelectRow
                    title={t.settings.models.pickerLabel}
                    desc={t.settings.models.pickerDesc}
                    value={summaryChoice?.file ?? ""}
                    onChange={(v) => patch({ local_summary_model: v })}
                    options={[
                      { value: "", label: t.settings.models.auto(summaryModel.auto.label) },
                      ...summaryModel.choices.map((c) => ({
                        value: c.file,
                        label: c.downloaded
                          ? `${c.label} · ${c.size}`
                          : `${c.label} · ${c.size} · ${t.settings.models.needsDownload}`,
                      })),
                    ]}
                    last
                  />
                  {summaryChoice && (!summaryChoice.downloaded || summaryChoice.exceeds_tier) && (
                    <div className="border-t border-line px-4 py-2.5 text-[11.5px]">
                      {!summaryChoice.downloaded && (
                        <p className="text-muted">{t.settings.models.willDownload(summaryChoice.size)}</p>
                      )}
                      {summaryChoice.exceeds_tier && (
                        <p className="text-amber">{t.settings.models.exceedsTier}</p>
                      )}
                    </div>
                  )}
                </>
              )}
            </div>
          </section>

          {/* ── 要約エンジン ── */}
          <section ref={refs.engine} className="mt-7 scroll-mt-6">
            <div className={SECTION_TITLE}>{t.settings.nav.engine}</div>
            <div className={SECTION_DESC}>{t.settings.engine.desc}</div>
            <div className="mt-3.5 flex gap-3">
              <EngineCard
                active={engine === "local"}
                onClick={() => patch({ engine: "local" })}
                title={t.settings.engine.local.title}
                badge={t.settings.engine.local.badge}
                badgeTone="green"
                desc={t.settings.engine.local.desc}
              />
              <EngineCard
                active={engine === "cloud"}
                onClick={() => patch({ engine: "cloud" })}
                title={t.settings.engine.cloud.title}
                badge={t.settings.engine.cloud.badge}
                badgeTone="indigo"
                desc={t.settings.engine.cloud.desc}
              />
            </div>

            {/* クラウド選択時のみ */}
            {engine === "cloud" && (
              <div className="mt-3.5 animate-mjfade rounded-card border border-border bg-surface p-4">
                <div className="flex gap-2.5">
                  <div className="w-[150px] shrink-0">
                    <div className="mb-1.5 text-[11.5px] text-sub">{t.settings.engine.provider}</div>
                    <div className="relative">
                      <select
                        value={provider}
                        onChange={(e) => changeProvider(e.target.value as Settings["provider"])}
                        className="w-full appearance-none rounded-btn border border-border-2 bg-surface-2 px-3 py-2.5 pr-8 text-[12.5px] text-body focus:border-brand focus:outline-none"
                      >
                        <option value="anthropic">Anthropic</option>
                        <option value="openai">OpenAI</option>
                      </select>
                      <ChevronDownIcon
                        size={14}
                        className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-faint"
                      />
                    </div>
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="mb-1.5 text-[11.5px] text-sub">
                      {t.settings.engine.model}{" "}
                      <span className="text-faint">{t.settings.engine.modelEmptyHint}</span>
                    </div>
                    <input
                      type="text"
                      value={cfg?.model ?? ""}
                      onChange={(e) => patch({ model: e.target.value })}
                      placeholder={MODEL_PLACEHOLDER[provider]}
                      className="w-full rounded-btn border border-border-2 bg-surface-2 px-3 py-2.5 font-mono text-[12.5px] text-body placeholder:text-faint focus:border-brand focus:outline-none"
                    />
                  </div>
                </div>

                {/* API キー（キーチェーン保管） */}
                <div className="mt-3">
                  <div className="mb-1.5 flex items-center gap-2 text-[11.5px] text-sub">
                    {t.settings.engine.apiKey}
                    {keySaved && (
                      <StatusBadge tone="green">{t.settings.engine.keySavedBadge}</StatusBadge>
                    )}
                  </div>
                  <div className="flex gap-2">
                    <input
                      type="password"
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      placeholder={
                        keySaved ? t.settings.engine.keySavedPlaceholder : KEY_PLACEHOLDER[provider]
                      }
                      onKeyDown={(e) => {
                        if (e.key === "Enter") saveKey();
                      }}
                      className="min-w-0 flex-1 rounded-btn border border-border-2 bg-surface-2 px-3 py-2.5 font-mono text-[12.5px] tracking-wide text-body placeholder:text-faint focus:border-brand focus:outline-none"
                    />
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={saveKey}
                      disabled={keyBusy || !apiKey.trim()}
                    >
                      {t.common.save}
                    </Button>
                    {keySaved && (
                      <Button size="sm" variant="secondary" onClick={clearKey} disabled={keyBusy}>
                        {t.common.delete}
                      </Button>
                    )}
                  </div>
                </div>

                <div className="mt-2.5 flex items-start gap-1.5">
                  <ShieldIcon size={12} className="mt-px shrink-0 text-amber" />
                  <span className="text-[11px] text-amber">
                    {t.settings.engine.cloudNotePre}
                    <strong className="font-semibold">
                      {t.settings.engine.cloudNoteStrong(PROVIDER_LABEL[provider])}
                    </strong>
                    {t.settings.engine.cloudNotePost}
                  </span>
                </div>
              </div>
            )}
          </section>

          {/* ── プライバシー ── */}
          <section ref={refs.privacy} className="mt-7 scroll-mt-6">
            <div className={SECTION_TITLE}>{t.settings.nav.privacy}</div>
            {engine === "cloud" ? (
              <div className="mt-3.5 flex items-start gap-2.5 rounded-card border border-amber/30 bg-amber/10 p-4">
                <ShieldIcon size={18} className="mt-px shrink-0 text-amber" />
                <div className="text-[12.5px] leading-relaxed text-body">
                  {t.settings.privacy.cloudIntro}
                  <strong className="font-semibold">{t.settings.privacy.cloudByokStrong}</strong>
                  {t.settings.privacy.cloudByokRest(PROVIDER_LABEL[provider])}
                  <strong className="font-semibold">{t.settings.privacy.cloudExportStrong}</strong>
                  {t.settings.privacy.cloudExportRest}
                  <strong className="font-semibold">{t.settings.privacy.cloudAiStrong}</strong>
                  {t.settings.privacy.cloudAiRest}
                </div>
              </div>
            ) : (
              <div className="mt-3.5 flex items-start gap-2.5 rounded-card border border-green/25 bg-green/10 p-4">
                <ShieldIcon size={18} className="mt-px shrink-0 text-green" />
                <div className="text-[12.5px] leading-relaxed text-body">
                  {t.settings.privacy.localIntro}
                  <strong className="font-semibold">{t.settings.privacy.localExportStrong}</strong>
                  {t.settings.privacy.localExportRest}
                  <strong className="font-semibold">{t.settings.privacy.localAiStrong}</strong>
                  {t.settings.privacy.localAiRest}
                </div>
              </div>
            )}
            <div className="mt-3 overflow-hidden rounded-card border border-border bg-surface">
              <ToggleRow
                title={t.settings.privacy.saveRecordings.title}
                desc={t.settings.privacy.saveRecordings.desc}
                checked={cfg?.save_recordings ?? true}
                onChange={(v) => patch({ save_recordings: v })}
              />
              <ToggleRow
                title={t.settings.privacy.sendUsage.title}
                desc={t.settings.privacy.sendUsage.desc}
                checked={cfg?.send_usage ?? false}
                onChange={(v) => patch({ send_usage: v })}
                last
              />
            </div>
            <div className="mt-2 text-[11px] text-faint">{t.settings.privacy.note}</div>
          </section>

          {/* ── 一般 ── */}
          <section ref={refs.general} className="mt-7 scroll-mt-6 pb-4">
            <div className={SECTION_TITLE}>{t.settings.nav.general}</div>
            <p className={SECTION_DESC}>{t.settings.general.desc}</p>

            {/* 言語（UI 言語 = コンテンツ言語 / 文字起こし言語） */}
            <div className="mt-3.5 overflow-hidden rounded-card border border-border bg-surface">
              <SelectRow
                title={t.settings.language.uiLabel}
                desc={t.settings.language.uiDesc}
                value={lang}
                onChange={(v) => setLang(v as "ja" | "en")}
                options={[
                  { value: "ja", label: t.settings.language.names.ja },
                  { value: "en", label: t.settings.language.names.en },
                ]}
              />
              <SelectRow
                title={t.settings.language.transcribeLabel}
                desc={t.settings.language.transcribeDesc}
                value={
                  cfg?.transcribe_language === "ja" || cfg?.transcribe_language === "en"
                    ? cfg.transcribe_language
                    : "auto"
                }
                onChange={(v) => patch({ transcribe_language: v as Settings["transcribe_language"] })}
                options={[
                  { value: "auto", label: t.settings.language.auto },
                  { value: "ja", label: t.settings.language.names.ja },
                  { value: "en", label: t.settings.language.names.en },
                ]}
                last
              />
            </div>

            {/* 会議開始の自動録音プロンプト（ADR-0026）。カレンダー連携が前提。 */}
            <div className="mt-3.5 rounded-card border border-border bg-surface">
              <ToggleRow
                title={t.settings.general.autoRecordPrompt.title}
                desc={t.settings.general.autoRecordPrompt.desc}
                checked={cfg?.auto_record_prompt ?? false}
                onChange={(v) => patch({ auto_record_prompt: v })}
                last
              />
            </div>

            <div className="mt-3.5 rounded-card border border-border bg-surface">
              {/* アプリ情報 */}
              <div className="flex items-center border-b border-line px-4 py-3.5">
                <div className="min-w-0 flex-1">
                  <div className="text-[13px] text-ink">mojiroku</div>
                  <div className="mt-0.5 text-[11px] text-muted">
                    {t.settings.general.version} <span className="font-mono">{appVersion || "—"}</span>
                  </div>
                </div>
              </div>
              {/* フィードバック導線 */}
              <div className="flex items-center px-4 py-3.5">
                <div className="min-w-0 flex-1">
                  <div className="text-[13px] text-ink">{t.settings.general.feedbackTitle}</div>
                  <div className="mt-0.5 text-[11px] text-muted">
                    {t.settings.general.feedbackDesc}
                  </div>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<MessageIcon size={14} />}
                  onClick={async () => {
                    try {
                      await openFeedbackForm();
                      toast(t.settings.general.feedbackOpened, "success");
                    } catch (e) {
                      toast(t.settings.general.feedbackOpenFailed(translateError(e, t)), "error");
                    }
                  }}
                >
                  {t.common.open}
                </Button>
              </div>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}

// ── モデル行 ──
function ModelRow({
  icon,
  tint,
  name,
  model,
  size,
  status,
  action,
  onAction,
  last,
}: {
  icon: ReactNode;
  tint: string;
  name: string;
  model: string;
  size: string;
  status: "saved" | "ondemand";
  action: string;
  onAction: () => void;
  last?: boolean;
}) {
  const { t } = useI18n();
  return (
    <div className={cx("flex items-center gap-3 px-4 py-3.5", !last && "border-b border-line")}>
      <span className={cx("flex h-9 w-9 shrink-0 items-center justify-center rounded-lg", tint)}>
        {icon}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[13.5px] font-semibold text-ink">{name}</div>
        <div className="mt-0.5 truncate text-[11.5px] text-muted">
          {model} · <span className="font-mono">{size}</span>
        </div>
      </div>
      {status === "saved" ? (
        <span className="rounded-md border border-green/25 bg-green/10 px-2.5 py-1 text-[11px] text-green">
          {t.settings.models.savedBadge}
        </span>
      ) : (
        <span className="rounded-md border border-border-2 bg-surface-2 px-2.5 py-1 text-[11px] text-sub">
          {t.settings.models.onDemandBadge}
        </span>
      )}
      <button
        onClick={onAction}
        className="text-[11.5px] text-sub transition-colors hover:text-ink"
      >
        {action}
      </button>
    </div>
  );
}

// ── 要約エンジンのラジオカード ──
function EngineCard({
  active,
  onClick,
  title,
  badge,
  badgeTone,
  desc,
}: {
  active: boolean;
  onClick: () => void;
  title: string;
  badge: string;
  badgeTone: "green" | "indigo";
  desc: string;
}) {
  return (
    <button
      onClick={onClick}
      className={cx(
        "flex-1 rounded-card border p-4 text-left transition-colors",
        active ? "border-brand bg-selected" : "border-border-2 bg-surface-2 hover:bg-hover",
      )}
    >
      <div className="flex items-center gap-2">
        <span
          className={cx(
            "flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border",
            active ? "border-brand" : "border-border-3",
          )}
        >
          {active && <span className="h-1.5 w-1.5 rounded-full bg-brand" />}
        </span>
        <span
          className={cx(
            "min-w-0 truncate text-[13.5px] font-bold",
            active ? "text-ink" : "text-body",
          )}
        >
          {title}
        </span>
        <span
          className={cx(
            "ml-auto shrink-0 whitespace-nowrap rounded-md px-2 py-0.5 text-[10.5px]",
            badgeTone === "green" ? "bg-green/12 text-green" : "bg-brand/15 text-brand-light",
          )}
        >
          {badge}
        </span>
      </div>
      <div className="mt-2 text-[11.5px] leading-relaxed text-sub">{desc}</div>
    </button>
  );
}

// ── セレクト行（言語設定など） ──
function SelectRow({
  title,
  desc,
  value,
  onChange,
  options,
  last,
  stacked = false,
}: {
  title: string;
  desc: string;
  value: string;
  onChange: (next: string) => void;
  options: { value: string; label: string }[];
  last?: boolean;
  stacked?: boolean;
}) {
  return (
    <div className={cx("flex gap-3 px-4 py-3.5", stacked ? "flex-col items-stretch" : "items-center", !last && "border-b border-line")}>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] text-ink">{title}</div>
        <div className="mt-0.5 text-[11px] text-muted">{desc}</div>
      </div>
      <div className="relative shrink-0">
        <select
          value={value}
          onChange={(e) => onChange(e.target.value)}
          aria-label={title}
          className={cx("max-w-full appearance-none rounded-btn border border-border-2 bg-surface-2 px-3 py-2 pr-8 text-[12.5px] text-body focus:border-brand focus:outline-none", stacked && "w-full")}
        >
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
        <ChevronDownIcon
          size={14}
          className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-faint"
        />
      </div>
    </div>
  );
}

// ── トグル行 ──
function ToggleRow({
  title,
  desc,
  checked,
  onChange,
  last,
}: {
  title: string;
  desc: string;
  checked: boolean;
  onChange: (next: boolean) => void;
  last?: boolean;
}) {
  return (
    <div className={cx("flex items-center px-4 py-3.5", !last && "border-b border-line")}>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] text-ink">{title}</div>
        <div className="mt-0.5 text-[11px] text-muted">{desc}</div>
      </div>
      <Toggle checked={checked} onChange={onChange} label={title} />
    </div>
  );
}
