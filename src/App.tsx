import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, engineHint, isTauri, readClipboard, writeClipboard } from "./bridge";
import { I18nProvider, detectSystemLang, resolveLang, setCurrentLang, translate, type Params } from "./i18n";
import type { Analysis, AppSettings, ConvertResult, Formats, GeneratedRecipe, OutputOptions, ParseOptions, Recipe, Step } from "./types";
import { buildRecipe, cleanOptions, cleanParse, defaultOptionsFor, recipeName, stateFromRecipe } from "./utils";
import InputPane from "./components/InputPane";
import OutputPane from "./components/OutputPane";
import ReportPanel from "./components/ReportPanel";
import StepsBar from "./components/StepsBar";
import RecipesPanel from "./components/RecipesPanel";
import AiPanel from "./components/AiPanel";
import SettingsModal from "./components/SettingsModal";

type Tab = "convert" | "recipes" | "ai";

const EMPTY_FORMATS: Formats = { inputs: [], outputs: [] };

export default function App() {
  const [tab, setTab] = useState<Tab>("convert");
  const [input, setInput] = useState("");
  const [inputFormat, setInputFormat] = useState("auto");
  const [parse, setParse] = useState<ParseOptions>({});
  const [steps, setSteps] = useState<Step[]>([]);
  const [outputFormat, setOutputFormat] = useState("");
  const [optionsByFormat, setOptionsByFormat] = useState<Record<string, OutputOptions>>({});
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [result, setResult] = useState<ConvertResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [formats, setFormats] = useState<Formats>(EMPTY_FORMATS);
  const [recipes, setRecipes] = useState<Recipe[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [activeRecipe, setActiveRecipe] = useState<Recipe | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [engineReady, setEngineReady] = useState(isTauri());

  const requestId = useRef(0);
  const lastClipboard = useRef<string>("");
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const lang = settings ? resolveLang(settings.language) : detectSystemLang();
  setCurrentLang(lang);
  const t = useCallback((key: string, params?: Params) => translate(lang, key, params), [lang]);

  const effectiveFormat = outputFormat || analysis?.suggestions[0]?.id || settings?.default_output || "json";
  const options = useMemo(() => optionsByFormat[effectiveFormat] ?? defaultOptionsFor(effectiveFormat), [optionsByFormat, effectiveFormat]);

  const showToast = useCallback((message: string) => {
    setToast(message);
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 2600);
  }, []);

  const refreshRecipes = useCallback(async () => {
    try {
      setRecipes(await api.listRecipes());
    } catch (e) {
      console.warn(e);
    }
  }, []);

  // Initial load -----------------------------------------------------------
  useEffect(() => {
    api
      .listFormats()
      .then((f) => {
        setFormats(f);
        setEngineReady(true);
      })
      .catch(console.warn);
    api.getSettings().then(setSettings).catch(console.warn);
    refreshRecipes();
  }, [refreshRecipes]);

  // Theme + language -----------------------------------------------------------
  useEffect(() => {
    const theme = settings?.theme ?? "system";
    if (theme === "light" || theme === "dark") {
      document.documentElement.dataset.theme = theme;
    } else {
      delete document.documentElement.dataset.theme;
    }
  }, [settings?.theme]);

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  // Analyze + convert whenever the inputs change ----------------------------
  useEffect(() => {
    if (!input.trim()) {
      setAnalysis(null);
      setResult(null);
      setError(null);
      return;
    }
    const id = ++requestId.current;
    const timer = setTimeout(async () => {
      setBusy(true);
      try {
        const a = await api.analyze(input, inputFormat, cleanParse(parse), lang);
        if (id !== requestId.current) return;
        setAnalysis(a);
        const fmt = outputFormat || a.suggestions[0]?.id || "json";
        const opts = optionsByFormat[fmt] ?? defaultOptionsFor(fmt);
        const r = await api.convert({
          input,
          input_format: inputFormat === "auto" ? undefined : inputFormat,
          parse: cleanParse(parse),
          steps,
          output: fmt,
          options: cleanOptions(opts),
          lang,
        });
        if (id !== requestId.current) return;
        setResult(r);
        setError(null);
      } catch (e) {
        if (id !== requestId.current) return;
        setError(e instanceof Error ? e.message : String(e));
        setResult(null);
      } finally {
        if (id === requestId.current) setBusy(false);
      }
    }, 180);
    return () => clearTimeout(timer);
  }, [input, inputFormat, parse, steps, outputFormat, optionsByFormat, lang]);

  // Auto copy -----------------------------------------------------------------
  useEffect(() => {
    if (settings?.auto_copy_result && result?.output) {
      writeClipboard(result.output).catch(() => undefined);
    }
  }, [result, settings?.auto_copy_result]);

  // Clipboard watching / paste on focus ---------------------------------------
  useEffect(() => {
    if (!settings?.watch_clipboard || !isTauri()) return;
    const timer = setInterval(async () => {
      try {
        const text = await readClipboard();
        if (text && text !== lastClipboard.current) {
          lastClipboard.current = text;
          setInput(text);
          setActiveRecipe(null);
        }
      } catch {
        /* clipboard may hold non-text */
      }
    }, 1000);
    return () => clearInterval(timer);
  }, [settings?.watch_clipboard]);

  useEffect(() => {
    if (!settings?.auto_paste_on_focus || !isTauri()) return;
    const onFocus = async () => {
      try {
        const text = await readClipboard();
        if (text && text !== lastClipboard.current) {
          lastClipboard.current = text;
          setInput(text);
        }
      } catch {
        /* ignore */
      }
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [settings?.auto_paste_on_focus]);

  // Handlers ------------------------------------------------------------------
  const pasteFromClipboard = useCallback(async () => {
    try {
      const text = await readClipboard();
      if (!text) {
        showToast(t("app.toast.clipboardEmpty"));
        return;
      }
      lastClipboard.current = text;
      setInput(text);
      setTab("convert");
    } catch (e) {
      showToast(e instanceof Error ? e.message : String(e));
    }
  }, [showToast, t]);

  const copyOutput = useCallback(async () => {
    if (!result?.output) return;
    try {
      await writeClipboard(result.output);
      lastClipboard.current = result.output;
      showToast(t("app.toast.copied"));
    } catch (e) {
      showToast(e instanceof Error ? e.message : String(e));
    }
  }, [result, showToast, t]);

  // Keyboard shortcuts ------------------------------------------------------------
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      if (e.key === "Enter" && result?.output) {
        e.preventDefault();
        copyOutput();
      } else if (e.key === ",") {
        e.preventDefault();
        setShowSettings(true);
      } else if (e.shiftKey && e.key.toLowerCase() === "v") {
        e.preventDefault();
        pasteFromClipboard();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [result, copyOutput, pasteFromClipboard]);

  const updateOptions = useCallback(
    (next: OutputOptions) => {
      setOptionsByFormat((prev) => ({ ...prev, [effectiveFormat]: next }));
    },
    [effectiveFormat],
  );

  const applyRecipe = useCallback(
    (recipe: Recipe, note = true) => {
      const state = stateFromRecipe(recipe);
      setInputFormat(state.inputFormat);
      setParse(state.parse);
      setSteps(state.steps);
      setOutputFormat(state.outputFormat);
      setOptionsByFormat((prev) => ({ ...prev, [state.outputFormat]: { ...defaultOptionsFor(state.outputFormat), ...state.options } }));
      setActiveRecipe(recipe);
      setTab("convert");
      if (note) showToast(t("app.toast.recipeLoaded", { name: recipeName(recipe, lang) }));
    },
    [showToast, t, lang],
  );

  const converterState = useMemo(
    () => ({ inputFormat, parse, steps, outputFormat: effectiveFormat, options }),
    [inputFormat, parse, steps, effectiveFormat, options],
  );

  const saveAsRecipe = useCallback(
    async (name: string) => {
      try {
        const saved = await api.saveRecipe(buildRecipe(name, converterState), lang);
        setActiveRecipe(saved);
        await refreshRecipes();
        showToast(t("app.toast.recipeSaved", { name: recipeName(saved, lang) }));
      } catch (e) {
        showToast(e instanceof Error ? e.message : String(e));
      }
    },
    [converterState, refreshRecipes, showToast, t, lang],
  );

  const updateActiveRecipe = useCallback(async () => {
    if (!activeRecipe || activeRecipe.builtin) return;
    try {
      const saved = await api.saveRecipe(buildRecipe(activeRecipe.name, converterState, activeRecipe), lang);
      setActiveRecipe(saved);
      await refreshRecipes();
      showToast(t("app.toast.recipeUpdated", { name: recipeName(saved, lang) }));
    } catch (e) {
      showToast(e instanceof Error ? e.message : String(e));
    }
  }, [activeRecipe, converterState, refreshRecipes, showToast, t, lang]);

  const onGenerated = useCallback(
    (g: GeneratedRecipe) => {
      applyRecipe(g.recipe, false);
      showToast(t("app.toast.generatedApplied"));
    },
    [applyRecipe, showToast, t],
  );

  const saveSettings = useCallback(
    async (next: AppSettings) => {
      try {
        const saved = await api.saveSettings(next);
        setSettings(saved);
        setShowSettings(false);
        showToast(translate(resolveLang(saved.language), "app.toast.settingsSaved"));
      } catch (e) {
        showToast(e instanceof Error ? e.message : String(e));
      }
    },
    [showToast],
  );

  const clearInput = useCallback(() => {
    setInput("");
    setActiveRecipe(null);
  }, []);

  return (
    <I18nProvider lang={lang}>
      <div className="app">
        <header className="header">
          <div className="brand">
            <span className="logo">{"{}"}</span>
            PasteCompiler
          </div>
          <nav className="tabs">
            <button className={`tab ${tab === "convert" ? "active" : ""}`} onClick={() => setTab("convert")}>
              {t("app.tab.convert")}
            </button>
            <button className={`tab ${tab === "recipes" ? "active" : ""}`} onClick={() => setTab("recipes")}>
              {t("app.tab.recipes")} {recipes.length ? <span className="muted small">({recipes.length})</span> : null}
            </button>
            <button className={`tab ${tab === "ai" ? "active" : ""}`} onClick={() => setTab("ai")}>
              {t("app.tab.describe")}
            </button>
          </nav>
          <div className="spacer" />
          <button className="btn primary" onClick={pasteFromClipboard} title={t("app.paste.title")}>
            {t("app.paste")}
          </button>
          <button className="btn" onClick={() => setShowSettings(true)} title={t("app.settings.title")}>
            {t("app.settings")}
          </button>
        </header>

        {!isTauri() && !engineReady && <div className="browser-banner">{engineHint()}</div>}

        <main className="main">
          {tab === "convert" && (
            <div className="convert">
              <div className="area-steps pane">
                <StepsBar
                  steps={steps}
                  activeRecipe={activeRecipe}
                  onRemove={(i) => setSteps((prev) => prev.filter((_, idx) => idx !== i))}
                  onClear={() => {
                    setSteps([]);
                    setActiveRecipe(null);
                  }}
                  onSaveAs={saveAsRecipe}
                  onUpdate={updateActiveRecipe}
                  onDescribe={() => setTab("ai")}
                />
              </div>
              <div className="area-input pane">
                <InputPane
                  input={input}
                  onInput={(v) => {
                    setInput(v);
                  }}
                  onClear={clearInput}
                  onPaste={pasteFromClipboard}
                  analysis={analysis}
                  inputFormat={inputFormat}
                  onInputFormat={setInputFormat}
                  inputFormats={formats.inputs}
                  parse={parse}
                  onParse={setParse}
                  busy={busy}
                />
              </div>
              <div className="area-output pane">
                <OutputPane
                  result={result}
                  error={error}
                  busy={busy}
                  outputs={formats.outputs}
                  suggestions={analysis?.suggestions ?? []}
                  outputFormat={effectiveFormat}
                  onFormat={setOutputFormat}
                  options={options}
                  onOptions={updateOptions}
                  onCopy={copyOutput}
                  onToast={showToast}
                />
              </div>
              <div className="area-report pane">
                <ReportPanel result={result} />
              </div>
            </div>
          )}

          {tab === "recipes" && (
            <RecipesPanel recipes={recipes} input={input} onApply={applyRecipe} onRefresh={refreshRecipes} onToast={showToast} />
          )}

          {tab === "ai" && (
            <AiPanel
              input={input}
              settings={settings}
              activeRecipe={activeRecipe}
              onApply={onGenerated}
              onSaved={refreshRecipes}
              onOpenSettings={() => setShowSettings(true)}
              onToast={showToast}
            />
          )}
        </main>

        {showSettings && settings && <SettingsModal settings={settings} onClose={() => setShowSettings(false)} onSave={saveSettings} />}
        {showSettings && !settings && (
          <div className="modal-backdrop" onClick={() => setShowSettings(false)}>
            <div className="modal" onClick={(e) => e.stopPropagation()}>
              <div className="modal-body">{engineHint()}</div>
            </div>
          </div>
        )}
        {toast && <div className="toast">{toast}</div>}
      </div>
    </I18nProvider>
  );
}
