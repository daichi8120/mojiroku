import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nCtx, dicts } from "@/i18n";
import { LiveTranslationPanel } from "./LiveTranslationPanel";
import type { useLiveTranslation } from "@/lib/useLiveTranslation";

const failedView = (unavailable: boolean): ReturnType<typeof useLiveTranslation> => ({
  enabled: true, starting: false, rows: [], progress: null, failed: true,
  unavailable, pending: 0, skipped: 0, start: vi.fn(), stop: vi.fn(), retry: vi.fn(),
});
const renderFailure = (unavailable: boolean) => renderToStaticMarkup(
  <I18nCtx.Provider value={{ lang: "en", t: dicts.en, setLang: vi.fn() }}>
    <LiveTranslationPanel translation={failedView(unavailable)} target="en" setTarget={vi.fn()} capturing />
  </I18nCtx.Provider>,
);

describe("live translation availability", () => {
  it("explains unsupported memory without offering a retry", () => {
    const html = renderFailure(true);
    expect(html).toContain("Live translation is unavailable on this Mac.");
    expect(html).toContain("at least 16 GB of RAM");
    expect(html).not.toContain("Retry translation");
  });
  it("offers retry for recoverable failures", () => {
    const html = renderFailure(false);
    expect(html).toContain("Translation paused.");
    expect(html).toContain("Retry translation");
  });
});
