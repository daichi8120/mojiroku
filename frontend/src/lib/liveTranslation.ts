import { LiveTranslationQueue, type TranslationRow, type TranslationSource } from "./liveTranslationQueue";

export type TranslationTarget = "ja" | "en";
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
}
interface Activity { session: TranslationSession; queue: LiveTranslationQueue; pumping: boolean; cancelledRequests: Set<number>; unlisten?: () => void }
const emptyView = (): TranslationView => ({ enabled: false, starting: false, rows: [], progress: null, failed: false, unavailable: false, pending: 0, skipped: 0 });

/** Owns one screen's temporary translations; every asynchronous callback is activity-scoped. */
export class LiveTranslationController {
  private generation = 0;
  private activity: Activity | null = null;
  private source: { sessionId: string; lines: readonly TranslationSource[] } | null = null;
  private view = emptyView();
  constructor(private transport: TranslationTransport, private changed: (view: TranslationView) => void) {}

  private publish(): void {
    if (this.activity) {
      this.view.rows = this.activity.queue.snapshot();
      this.view.pending = this.activity.queue.pendingCount();
      this.view.skipped = this.activity.queue.skippedCount();
    }
    this.changed({ ...this.view });
  }

  async start(target: TranslationTarget): Promise<void> {
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
      this.activity = { session, queue: new LiveTranslationQueue(), pumping: false, cancelledRequests: new Set() };
      this.view.starting = false;
      if (this.source?.sessionId === session.session_id) this.activity.queue.update(this.source.lines);
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
          active.queue.finish(request, result.text);
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
