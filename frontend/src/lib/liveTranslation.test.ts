import { describe, expect, it, vi } from "vitest";
import { LiveTranslationController, type TranslationProgress, type TranslationResult, type TranslationSession, type TranslationTransport, type TranslationView } from "./liveTranslation";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function flush() { for (let i = 0; i < 12; i++) await Promise.resolve(); }
const session = (epoch = 1): TranslationSession => ({ epoch, session_id: "meeting-a", model_bytes: 100 });
const line = (text = "source caption") => ({ id: 1, text, committed: false });
function setup(limits?: { maxRows: number; maxBytes: number }) {
  const calls: { epoch: number; requestId: number; text: string; result: ReturnType<typeof deferred<TranslationResult>> }[] = [];
  const listeners = new Map<string, (progress: TranslationProgress) => void>();
  const unlisten = vi.fn();
  const transport: TranslationTransport = {
    begin: vi.fn(async () => session()),
    end: vi.fn(async () => {}), cancel: vi.fn(async () => {}),
    listen: vi.fn(async (event, handler) => {
      listeners.set(event, handler);
      return () => { unlisten(); listeners.delete(event); };
    }),
    translate: vi.fn((epoch, requestId, text) => {
      const result = deferred<TranslationResult>();
      calls.push({ epoch, requestId, text, result });
      return result.promise;
    }),
  };
  let view!: TranslationView;
  const controller = new LiveTranslationController(transport, (next) => { view = next; }, limits);
  const complete = (index: number, text: string) => {
    const call = calls[index];
    call.result.resolve({ epoch: call.epoch, request_id: call.requestId, text, elapsed_ms: 10 });
  };
  return { controller, transport, calls, listeners, unlisten, complete, view: () => view };
}

describe("live translation lifecycle", () => {
  it("rejects malformed output without putting an unsaveable row in history", async () => {
    for (const text of ["   ", "x".repeat(16_385), "\u3042".repeat(6000)]) {
      const x = setup();
      x.controller.update("meeting-a", [line()]);
      await x.controller.start("ja"); await flush();
      x.complete(0, text); await flush();
      expect(x.controller.completed()).toEqual([]);
      expect(x.view().failed).toBe(true);
      x.controller.stop();
      expect(x.controller.completed()).toEqual([]);
    }
  });

  it("pauses translation before its history would exceed the save budget", async () => {
    for (const limits of [{ maxRows: 1, maxBytes: 1000 }, { maxRows: 10, maxBytes: 4 }]) {
      const x = setup(limits);
      x.controller.update("meeting-a", [line("a")]);
      await x.controller.start("ja"); await flush();
      x.complete(0, "b"); await flush();
      x.controller.update("meeting-a", [{ id: 2, text: "cc", committed: true }]);
      await flush(); x.complete(1, "dd"); await flush();
      expect(x.view().historyFull).toBe(true);
      expect(x.view().enabled).toBe(false);
      expect(x.controller.completed()).toEqual([{ source_id: 1, source_text: "a", target: "ja", translation: "b" }]);
      x.controller.stop();
      expect(x.controller.completed()).toHaveLength(1);
      await x.controller.start("ja");
      expect(x.transport.begin).toHaveBeenCalledTimes(1);
      x.controller.reset();
      expect(x.view().historyFull).toBe(false);
    }
  });

  it("retains completed captions after disable, target change, and queue eviction", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("ja");
    await flush();
    x.complete(0, "completed Japanese caption");
    await flush();
    x.controller.update("meeting-a", [{ id: 90, text: "later caption", committed: true }]);
    x.controller.stop();
    expect(x.controller.completed()).toEqual([{ source_id: 1, source_text: "source caption", target: "ja", translation: "completed Japanese caption" }]);
    expect(x.view().rows[0].translation).toBe("completed Japanese caption");
    x.transport.begin = async () => session(2);
    await x.controller.start("en");
    await flush();
    x.complete(x.calls.length - 1, "completed English caption");
    await flush();
    expect(x.controller.completed()).toHaveLength(2);
    x.controller.reset();
    expect(x.controller.completed()).toEqual([]);
    expect(x.view().rows).toEqual([]);
  });

  it("reuses saved captions when translation is turned back on", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("en");
    await flush();
    x.complete(0, "saved caption");
    await flush();
    x.controller.stop();
    x.transport.begin = async () => session(2);
    await x.controller.start("en");
    await flush();
    expect(x.calls).toHaveLength(1);
    expect(x.view().rows[0].translation).toBe("saved caption");
  });

  it("does not save a late completion after Stop", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("ja");
    await flush();
    x.controller.stop();
    x.complete(0, "late result");
    await flush();
    expect(x.controller.completed()).toEqual([]);
  });

  it("ends a late begin response after the screen stopped", async () => {
    const x = setup();
    const begin = deferred<TranslationSession>();
    x.transport.begin = () => begin.promise;
    const starting = x.controller.start("en");
    x.controller.stop();
    begin.resolve(session(7));
    await starting;
    expect(x.transport.end).toHaveBeenCalledWith(7);
    expect(x.view().enabled).toBe(false);
    expect(x.calls).toHaveLength(0);
  });

  it("marks unsupported memory as unavailable and does not retry or translate", async () => {
    const x = setup();
    x.transport.begin = vi.fn(async () => { throw "translation.requires_16gb"; });
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("ja");
    expect(x.view().unavailable).toBe(true);
    expect(x.view().failed).toBe(true);
    expect(x.view().starting).toBe(false);
    x.controller.retry("ja");
    await flush();
    expect(x.transport.begin).toHaveBeenCalledOnce();
    expect(x.calls).toHaveLength(0);
    expect(x.listeners.size).toBe(0);
    x.controller.stop();
    expect(x.view().unavailable).toBe(false);
  });

  it("allows retry after a generic start error", async () => {
    const x = setup();
    x.transport.begin = vi.fn(async () => { throw "translation.start_failed"; });
    await x.controller.start("en");
    expect(x.view().failed).toBe(true);
    expect(x.view().unavailable).toBe(false);
    x.transport.begin = vi.fn(async () => session());
    x.controller.retry("en");
    await flush();
    expect(x.transport.begin).toHaveBeenCalledOnce();
    expect(x.view().failed).toBe(false);
  });

  it("ignores a previous meeting snapshot", async () => {
    const x = setup();
    x.controller.update("old-meeting", [line()]);
    await x.controller.start("ja");
    await flush();
    expect(x.calls).toHaveLength(0);
    x.controller.update("meeting-a", [line()]);
    await flush();
    expect(x.calls).toHaveLength(1);
    x.complete(0, "translated");
    await flush();
    expect(x.view().rows[0].translation).toBe("translated");
    expect(x.unlisten).toHaveBeenCalledOnce();
  });

  it("cancels revised drafts, keeps download progress, and discards late text", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line("old")]);
    await x.controller.start("ja");
    await flush();
    const first = x.calls[0];
    const progress = x.listeners.get(`translation://progress/1/${first.requestId}`)!;
    x.controller.update("meeting-a", [line("new")]);
    expect(x.transport.cancel).toHaveBeenCalledWith(1, first.requestId);
    progress({ stage: "download", done: 50, total: 100 });
    expect(x.view().progress?.done).toBe(50);
    progress({ stage: "translate", done: 1, total: null });
    expect(x.view().progress?.stage).toBe("download");
    x.complete(0, "stale text");
    await flush();
    expect(x.calls).toHaveLength(2);
    expect(x.calls[1].text).toBe("new");
    expect(x.view().rows[0].translation).toBeNull();
    x.complete(1, "fresh text");
    await flush();
    expect(x.view().rows[0].translation).toBe("fresh text");
  });

  it("disposes old listeners and ignores old failures after target change", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("ja");
    await flush();
    const oldProgress = [...x.listeners.values()][0];
    x.transport.begin = async () => session(2);
    await x.controller.start("en");
    await flush();
    expect(x.transport.end).toHaveBeenCalledWith(1);
    oldProgress({ stage: "queued", done: 0, total: null });
    expect(x.view().progress).toBeNull();
    x.calls[0].result.reject("cancelled");
    await flush();
    expect(x.view().failed).toBe(false);
    x.complete(1, "new target");
    await flush();
    expect(x.view().rows[0].translation).toBe("new target");
    expect(x.listeners.size).toBe(0);
  });

  it("pauses on a current failure and retries without restarting the model session", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("ja");
    await flush();
    x.calls[0].result.reject("private filesystem details");
    await flush();
    expect(x.view().failed).toBe(true);
    expect(x.view().rows[0].error).toBe("translation_failed");
    x.controller.update("meeting-a", [line()]);
    await flush();
    expect(x.calls).toHaveLength(1);
    x.controller.retry("ja");
    await flush();
    expect(x.calls).toHaveLength(2);
    expect(x.transport.begin).toHaveBeenCalledOnce();
    x.complete(1, "recovered");
    await flush();
    expect(x.view().failed).toBe(false);
    expect(x.view().rows[0].translation).toBe("recovered");
  });

  it("does not submit a request when disabled during listener registration", async () => {
    const x = setup();
    const registered = deferred<() => void>();
    x.transport.listen = () => registered.promise;
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("ja");
    x.controller.stop();
    registered.resolve(x.unlisten);
    await flush();
    expect(x.calls).toHaveLength(0);
    expect(x.unlisten).toHaveBeenCalledOnce();
    expect(x.view().rows).toHaveLength(0);
  });

  it("pauses on a download failure even after the source caption changed", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line("old")]);
    await x.controller.start("ja");
    await flush();
    x.controller.update("meeting-a", [line("new")]);
    x.calls[0].result.reject("download failed");
    await flush();
    expect(x.view().failed).toBe(true);
    expect(x.calls).toHaveLength(1);
    x.controller.retry("ja");
    await flush();
    expect(x.calls[1].text).toBe("new");
  });

  it("removes the progress listener immediately on stop before the process exits", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("ja");
    await flush();
    expect(x.listeners.size).toBe(1);
    x.controller.stop();
    expect(x.listeners.size).toBe(0);
    x.calls[0].result.reject("translation.cancelled");
    await flush();
    expect(x.unlisten).toHaveBeenCalledOnce();
  });

  it("retries a draft that returns to its original text after cancellation was requested", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line("original")]);
    await x.controller.start("ja");
    await flush();
    x.controller.update("meeting-a", [line("revision")]);
    x.controller.update("meeting-a", [line("original")]);
    x.calls[0].result.reject("translation.cancelled");
    await flush();
    expect(x.view().failed).toBe(false);
    expect(x.calls).toHaveLength(2);
    expect(x.calls[1].text).toBe("original");
    x.complete(1, "fresh");
    await flush();
    expect(x.view().rows[0].translation).toBe("fresh");
  });

  it("rejects a response with a mismatched request identity", async () => {
    const x = setup();
    x.controller.update("meeting-a", [line()]);
    await x.controller.start("ja");
    await flush();
    x.calls[0].result.resolve({ epoch: 9, request_id: 1, text: "wrong session", elapsed_ms: 1 });
    await flush();
    expect(x.view().failed).toBe(true);
    expect(x.view().rows[0].translation).toBeNull();
  });
});
