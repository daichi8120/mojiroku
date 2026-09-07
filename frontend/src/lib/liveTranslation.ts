import { LiveTranslationQueue, type TranslationRow, type TranslationSource } from "./liveTranslationQueue";

export type TranslationTarget = "ja" | "en";
export interface SavedLiveTranslation {
  source_id: number;
  source_text: string;
  target: TranslationTarget;
  translation: string;
}
export interface TranslationSession { epoch: number; session_id: string; model_bytes: number }
export interface TranslationProgress { stage: string; done: number; total: number | null }
export interface TranslationResult { epoch: number; request_id: number; text: string; elapsed_ms: number }
export interface TranslationTransport {
  begin(target: TranslationTarget): Promise<TranslationSession>;
  end(epoch: number): Promise<unknown>;
  cancel(epoch: number, requestId: number): Promise<unknown>;
  listen(event: string, handler: (progress: TranslationProgress) => void): Promise<() => void>;
  translate(epoch: number, requestId: number, text: string): Promise<TranslationResult>;
}
export interface TranslationView {
  enabled: boolean;
  starting: boolean;
  rows: TranslationRow[];
  progress: TranslationProgress | null;
  failed: boolean;
  unavailable: boolean;
  pending: number;
  skipped: number;
  historyFull: boolean;
}
interface Activity { target: TranslationTarget; session: TranslationSession; queue: LiveTranslationQueue; pumping: boolean; cancelledRequests: Set<number>; unlisten?: () => void }
const emptyView = (): TranslationView => ({ enabled: false, starting: false, rows: [], progress: null, failed: false, unavailable: false, pending: 0, skipped: 0, historyFull: false });

/** Owned by the app for the whole meeting; callbacks remain activity-scoped. */
export class LiveTranslationController {
  private generation = 0;
  private archiveBytes = 0;
  private historyFull = false;
  private archive = new Map<string, SavedLiveTranslation>();
  private activity: Activity | null = null;
  private source: { sessionId: string; lines: readonly TranslationSource[] } | null = null;
  private view = emptyView();
  constructor(private transport: TranslationTransport, private changed: (view: TranslationView) => void,
    private limits = { maxRows: 20_000, maxBytes: 32 * 1024 * 1024 }) {}

  private publish(): void {
    const activeRows = this.activity?.queue.snapshot() ?? [];
    const target = this.activity?.target;
    const visibleKeys = new Set(activeRows.map((row) => `${row.sourceId}:${target}`));
    const retained = [...this.archive.values()].filter((row) => !visibleKeys.has(`${row.source_id}:${row.target}`));
    this.view.rows = [
      ...retained.map((row) => ({ sourceId: row.source_id, sourceText: row.source_text, target: row.target,
        translation: row.translation, status: "ready" as const, committed: true, error: null })),
      ...activeRows.map((row) => ({ ...row, target })),
    ].sort((a, b) => a.sourceId - b.sourceId);
    this.view.pending = this.activity?.queue.pendingCount() ?? 0;
    this.view.skipped = this.activity?.queue.skippedCount() ?? 0;
    this.view.historyFull = this.historyFull;
    this.changed({ ...this.view });
  }

  async start(target: TranslationTarget): Promise<void> {
    if (this.historyFull) return;
    this.stop();
    const generation = this.generation;
    this.view = { ...emptyView(), enabled: true, starting: true };
    this.publish();
    try {
      const session = await this.transport.begin(target);
      if (generation !== this.generation) {
        void this.transport.end(session.epoch).catch(() => {});
        return;
      }
      this.activity = { target, session, queue: new LiveTranslationQueue(), pumping: false, cancelledRequests: new Set() };
      this.view.starting = false;
      if (this.source?.sessionId === session.session_id) this.activity.queue.update(this.source.lines);
      this.activity.queue.restore(this.completed().filter((row) => row.target === target));
      this.publish();
      void this.pump(this.activity);
    } catch (error) {
      if (generation !== this.generation) return;
      this.view.starting = false;
      this.view.failed = true;
      this.view.unavailable = String(error) === "translation.requires_16gb";
      this.publish();
    }
  }

  update(sessionId: string, lines: readonly TranslationSource[]): void {
    this.source = { sessionId, lines };
    const active = this.activity;
    if (!active || active.session.session_id !== sessionId) return;
    active.queue.update(lines);
    const request = active.queue.inFlight();
    if (request && !active.queue.isCurrent(request) && !active.cancelledRequests.has(request.requestId)) {
      active.cancelledRequests.add(request.requestId);
      void this.transport.cancel(active.session.epoch, request.requestId).catch(() => {});
    }
    this.publish();
    void this.pump(active);
  }

  stop(): void {
    ++this.generation;
    const active = this.activity;
    this.activity = null;
    if (active) {
      active.unlisten?.();
      void this.transport.end(active.session.epoch).catch(() => {});
    }
    this.view = emptyView();
    this.publish();
  }

  completed(): SavedLiveTranslation[] { return [...this.archive.values()].map((row) => ({ ...row })); }

  reset(): void {
    this.stop();
    this.archive.clear();
    this.archiveBytes = 0;
    this.historyFull = false;
    this.source = null;
    this.publish();
  }

  retry(target: TranslationTarget): void {
    if (this.view.unavailable) return;
    if (!this.activity) { void this.start(target); return; }
    this.view.failed = false;
    this.activity.queue.retryErrors();
    this.publish();
    void this.pump(this.activity);
  }

  private async pump(active: Activity): Promise<void> {
    if (active !== this.activity || active.pumping || this.view.failed) return;
    active.pumping = true;
    try {
      while (active === this.activity && !this.view.failed) {
        const request = active.queue.claim();
        if (!request) break;
        this.view.progress = null;
        this.publish();
        let unlisten: (() => void) | undefined;
        try {
          const dispose = await this.transport.listen(
            `translation://progress/${active.session.epoch}/${request.requestId}`,
            (progress) => {
              if (active !== this.activity || (progress.stage !== "download" && !active.queue.isCurrent(request))) return;
              this.view.progress = progress;
              this.publish();
            },
          );
          let disposed = false;
          unlisten = () => { if (!disposed) { disposed = true; dispose(); } };
          active.unlisten = unlisten;
          if (active !== this.activity || !active.queue.isCurrent(request)) {
            active.queue.finish(request, null);
            continue;
          }
          const result = await this.transport.translate(active.session.epoch, request.requestId, request.text);
          if (active !== this.activity) return;
          if (result.epoch !== active.session.epoch || result.request_id !== request.requestId) {
            throw new Error("Mismatched translation response");
          }
          if (!active.queue.isCurrent(request)) {
            active.queue.finish(request, null);
            continue;
          }
          // The native transport enforces this too; keep persisted history valid at its boundary.
          const bytes = (text: string) => new TextEncoder().encode(text).length;
          if (!result.text.trim() || bytes(result.text) > 16_384) {
            throw new Error("Invalid translation output");
          }
          const key = `${request.sourceId}:${active.target}`;
          const previous = this.archive.get(key);
          const total = this.archiveBytes + bytes(request.text) + bytes(result.text)
            - (previous ? bytes(previous.source_text) + bytes(previous.translation) : 0);
          if ((!previous && this.archive.size >= this.limits.maxRows) || total > this.limits.maxBytes) {
            this.historyFull = true;
            this.stop();
            break;
          }
          if (active.queue.finish(request, result.text)) {
            this.archive.set(key, {
              source_id: request.sourceId, source_text: request.text, target: active.target, translation: result.text,
            });
            this.archiveBytes = total;
          }
        } catch (error) {
          if (active !== this.activity) return;
          const current = active.queue.isCurrent(request);
          active.queue.finish(request, null, "translation_failed");
          // A model download can fail after its source caption was revised.
          // Pause that activity as well; only stale cancellation is expected.
          const expectedCancellation = String(error) === "translation.cancelled" && active.cancelledRequests.has(request.requestId);
          if (current && expectedCancellation) active.queue.retryErrors();
          else if (current || String(error) !== "translation.cancelled") this.view.failed = true;
        } finally {
          unlisten?.();
          active.unlisten = undefined;
          active.cancelledRequests.delete(request.requestId);
          if (active === this.activity) {
            this.view.progress = null;
            this.publish();
          }
        }
      }
    } finally {
      active.pumping = false;
    }
  }
}
