import { describe, expect, it } from "vitest";
import { LiveTranslationQueue } from "./liveTranslationQueue";

const line = (id: number, text = `caption ${id}`, committed = true) => ({ id, text, committed });

describe("live translation queue", () => {
  it("does not translate an unchanged caption twice", () => {
    const queue = new LiveTranslationQueue();
    queue.update([line(1)]);
    const request = queue.claim()!;
    queue.update([line(1)]);
    expect(queue.claim()).toBeNull();
    expect(queue.finish(request, "translated")).toBe(true);
    queue.update([line(1)]);
    expect(queue.claim()).toBeNull();
    expect(queue.snapshot()[0].translation).toBe("translated");
  });

  it("rejects stale results after a draft revision", () => {
    const queue = new LiveTranslationQueue();
    queue.update([line(1, "old draft", false)]);
    const old = queue.claim()!;
    queue.update([line(1, "new draft", false)]);
    expect(queue.isCurrent(old)).toBe(false);
    expect(queue.finish(old, "stale")).toBe(false);
    const current = queue.claim()!;
    expect(current.text).toBe("new draft");
    expect(queue.finish(current, "current")).toBe(true);
    expect(queue.snapshot()[0].translation).toBe("current");
  });

  it("keeps a valid translation when a draft becomes committed", () => {
    const queue = new LiveTranslationQueue();
    queue.update([line(1, "same words", false)]);
    const request = queue.claim()!;
    queue.update([line(1, "same words", true)]);
    expect(queue.isCurrent(request)).toBe(true);
    queue.finish(request, "translation");
    expect(queue.snapshot()[0].committed).toBe(true);
    expect(queue.claim()).toBeNull();
  });

  it("bounds pending work and keeps recent captions", () => {
    const queue = new LiveTranslationQueue();
    queue.update(Array.from({ length: 20 }, (_, i) => line(i)));
    expect(queue.pendingCount()).toBe(4);
    expect(queue.skippedCount()).toBe(16);
    expect(queue.claim()!.sourceId).toBe(16);
    queue.update(Array.from({ length: 200 }, (_, i) => line(i)));
    expect(queue.snapshot()).toHaveLength(80);
    expect(queue.pendingCount()).toBe(4);
  });

  it("ignores results from before stop/restart even when text repeats", () => {
    const queue = new LiveTranslationQueue();
    queue.update([line(1)]);
    const old = queue.claim()!;
    queue.reset();
    queue.update([line(1)]);
    const current = queue.claim()!;
    expect(queue.finish(old, "stale")).toBe(false);
    expect(queue.finish(current, "fresh")).toBe(true);
    expect(queue.snapshot()[0].translation).toBe("fresh");
  });

  it("rejects oversized UTF-8 input without silently truncating", () => {
    const queue = new LiveTranslationQueue();
    queue.update([line(1, "\u{1f600}".repeat(300)), line(2, "")]);
    expect(queue.claim()).toBeNull();
    expect(queue.snapshot()).toHaveLength(1);
    expect(queue.snapshot()[0].error).toBe("input_too_long");
  });
});
