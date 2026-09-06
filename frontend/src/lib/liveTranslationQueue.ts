export interface TranslationSource {
  id: number;
  text: string;
  committed: boolean;
}

export type TranslationRowStatus = "queued" | "translating" | "ready" | "error" | "skipped";

export interface TranslationRow {
  sourceId: number;
  sourceText: string;
  committed: boolean;
  status: TranslationRowStatus;
  translation: string | null;
  error: string | null;
}

export interface TranslationRequest {
  requestId: number;
  sourceId: number;
  text: string;
}

export const MAX_TRANSLATION_INPUT_BYTES = 1024;
const MAX_PENDING = 4;
const MAX_ROWS = 80;

/** One in-flight request plus four recent pending captions. Revisions replace pending work. */
export class LiveTranslationQueue {
  private rows: TranslationRow[] = [];
  private current: TranslationRequest | null = null;
  private sequence = 0;

  update(source: readonly TranslationSource[]): void {
    const previous = new Map(this.rows.map((row) => [row.sourceId, row]));
    this.rows = source.slice(-MAX_ROWS).filter((line) => line.text.trim()).map((line) => {
      const old = previous.get(line.id);
      if (old && old.sourceText === line.text) return { ...old, committed: line.committed };
      const tooLong = new TextEncoder().encode(line.text).byteLength > MAX_TRANSLATION_INPUT_BYTES;
      return {
        sourceId: line.id, sourceText: line.text, committed: line.committed,
        status: tooLong ? "error" : "queued", translation: null,
        error: tooLong ? "input_too_long" : null,
      };
    });
    const pending = this.rows.filter((row) => row.status === "queued");
    for (const row of pending.slice(0, Math.max(0, pending.length - MAX_PENDING))) {
      row.status = "skipped";
    }
  }

  claim(): TranslationRequest | null {
    if (this.current) return null;
    const row = this.rows.find((candidate) => candidate.status === "queued");
    if (!row) return null;
    row.status = "translating";
    this.current = { requestId: ++this.sequence, sourceId: row.sourceId, text: row.sourceText };
    return { ...this.current };
  }

  isCurrent(request: TranslationRequest): boolean {
    return this.current?.requestId === request.requestId && this.rows.some(
      (row) => row.sourceId === request.sourceId && row.sourceText === request.text,
    );
  }

  finish(request: TranslationRequest, translation: string | null, error: string | null = null): boolean {
    if (this.current?.requestId !== request.requestId) return false;
    const row = this.rows.find((candidate) => candidate.sourceId === request.sourceId && candidate.sourceText === request.text);
    this.current = null;
    if (!row) return false;
    row.translation = translation;
    row.error = error;
    row.status = error ? "error" : "ready";
    return true;
  }

  retryErrors(): void {
    for (const row of this.rows) {
      if (row.status === "error" && row.error !== "input_too_long") {
        row.status = "queued";
        row.error = null;
      }
    }
    // Reapply the same backlog bound after retrying failed work.
    this.update(this.rows.map((row) => ({ id: row.sourceId, text: row.sourceText, committed: row.committed })));
  }

  reset(): void {
    this.rows = [];
    this.current = null;
  }

  snapshot(): TranslationRow[] { return this.rows.map((row) => ({ ...row })); }
  inFlight(): TranslationRequest | null { return this.current ? { ...this.current } : null; }
  pendingCount(): number { return this.rows.filter((row) => row.status === "queued").length; }
  skippedCount(): number { return this.rows.filter((row) => row.status === "skipped").length; }
}
