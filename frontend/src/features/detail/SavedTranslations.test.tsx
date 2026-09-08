import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nCtx, dicts } from "@/i18n";
import { SavedTranslationRows } from "./SavedTranslations";

describe("saved live translations", () => {
  it("shows the original live caption snapshot separately from translated text", () => {
    const html = renderToStaticMarkup(<I18nCtx.Provider value={{ lang: "en", t: dicts.en, setLang: vi.fn() }}>
      <SavedTranslationRows rows={[
        { source_id: 7, source_text: "Original live wording", target: "ja", translation: "Saved translated wording" },
      ]} />
    </I18nCtx.Provider>);
    expect(html).toContain("Original live wording");
    expect(html).toContain("Saved translated wording");
    expect(html).toContain("Japanese");
    expect(html).toContain("wording may differ from the final transcript");
  });
});
