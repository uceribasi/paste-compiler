import { afterEach, describe, expect, it, vi } from "vitest";
import { detectSystemLang, getCurrentLang, hasKey, resolveLang, setCurrentLang, translate } from "./i18n";

describe("language resolution", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("detects Turkish from the browser locale", () => {
    vi.stubGlobal("navigator", { language: "tr-TR" });
    expect(detectSystemLang()).toBe("tr");
    expect(resolveLang("system")).toBe("tr");
    expect(resolveLang(undefined)).toBe("tr");
    vi.stubGlobal("navigator", { language: "en-GB" });
    expect(detectSystemLang()).toBe("en");
    expect(resolveLang(null)).toBe("en");
  });

  it("honours explicit settings", () => {
    expect(resolveLang("tr")).toBe("tr");
    expect(resolveLang("en")).toBe("en");
    expect(resolveLang("fr")).toBe(detectSystemLang());
  });

  it("keeps a module-level current language", () => {
    setCurrentLang("tr");
    expect(getCurrentLang()).toBe("tr");
    setCurrentLang("en");
    expect(getCurrentLang()).toBe("en");
  });
});

describe("translate", () => {
  it("substitutes placeholders", () => {
    expect(translate("en", "app.toast.recipeLoaded", { name: "X" })).toBe("Recipe “X” loaded");
    expect(translate("tr", "app.toast.recipeLoaded", { name: "X" })).toBe("“X” tarifi yüklendi");
    expect(translate("en", "common.rows", { a: 2, b: 3 })).toBe("2 → 3 rows");
  });

  it("falls back to English and then to the key", () => {
    expect(translate("tr", "definitely.missing")).toBe("definitely.missing");
    expect(translate("tr", "fmt.json")).toBe("JSON");
    expect(hasKey("app.tab.convert")).toBe(true);
    expect(hasKey("nope")).toBe(false);
  });

  it("leaves unknown placeholders untouched", () => {
    expect(translate("en", "opt.template.help")).toContain("{column}");
  });
});
