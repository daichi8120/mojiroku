import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nCtx, dicts } from "@/i18n";
import type { ModelDownload } from "@/lib/modelDownloads";
import { DownloadControl } from "./SettingsView";

const model: ModelDownload = {
  file: "fixture.gguf", size_bytes: 100, downloaded_bytes: 0, status: "missing", error: null,
};
function render(locale: "en" | "ja", value?: ModelDownload) {
  return renderToStaticMarkup(
    <I18nCtx.Provider value={{ lang: locale, t: dicts[locale], setLang: vi.fn() }}>
      <DownloadControl model={value} />
    </I18nCtx.Provider>,
  );
}

describe("model download control localization", () => {
  for (const locale of ["en", "ja"] as const) {
    it(`uses the ${locale} dictionary for loading, download, progress and retry`, () => {
      const copy = dicts[locale];
      expect(render(locale)).toContain(copy.common.loading);
      expect(render(locale, model)).toContain(`${copy.settings.models.fetch} fixture.gguf`);
      const progress = render(locale, { ...model, status: "downloading", downloaded_bytes: 40 });
      expect(progress).toContain(copy.job.stages.download);
      expect(progress).toContain("40%");
      expect(progress).toContain("disabled");
      expect(render(locale, { ...model, downloaded_bytes: 40 })).toContain(copy.common.retry);
    });
  }
});
