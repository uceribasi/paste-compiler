import { useEffect, useState } from "react";
import { api } from "../bridge";
import { resolveLang, translate, type Params } from "../i18n";
import type { AppPaths, AppSettings, Provider } from "../types";
import { PROVIDER_DEFAULTS, providerLabel } from "../utils";

interface Props {
  settings: AppSettings;
  onClose: () => void;
  onSave: (settings: AppSettings) => void;
}

const PROVIDERS: Provider[] = ["rules", "ollama", "openai_compatible", "anthropic"];

export default function SettingsModal({ settings, onClose, onSave }: Props) {
  const [local, setLocal] = useState<AppSettings>({ ...settings, language: settings.language ?? "system" });
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);
  const [paths, setPaths] = useState<AppPaths | null>(null);

  // The dialog previews the language the user is choosing.
  const lang = resolveLang(local.language);
  const t = (key: string, params?: Params) => translate(lang, key, params);

  useEffect(() => {
    api.appPaths().then(setPaths).catch(() => undefined);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const setAi = (patch: Partial<AppSettings["ai"]>) => setLocal((s) => ({ ...s, ai: { ...s.ai, ...patch } }));
  const defaults = PROVIDER_DEFAULTS[local.ai.provider];

  const test = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const message = await api.aiTestConnection(local.ai, lang);
      setTestResult({ ok: true, message });
    } catch (e) {
      setTestResult({ ok: false, message: e instanceof Error ? e.message : String(e) });
    } finally {
      setTesting(false);
    }
  };

  const ollamaHint = t("settings.ai.ollamaHint");
  const ollamaParts = ollamaHint.split(/(\{pull\}|\{serve\})/);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-label={t("settings.title")}>
        <div className="modal-header">
          <h2>{t("settings.title")}</h2>
          <button className="btn small ghost" onClick={onClose}>
            {t("common.close")}
          </button>
        </div>
        <div className="modal-body">
          <fieldset>
            <legend>{t("settings.ai.legend")}</legend>
            <div className="form-grid">
              <label className="field wide">
                <span>{t("settings.ai.provider")}</span>
                <select
                  value={local.ai.provider}
                  onChange={(e) => {
                    const provider = e.target.value as Provider;
                    setAi({ provider, base_url: "", model: "" });
                    setTestResult(null);
                  }}
                >
                  {PROVIDERS.map((p) => (
                    <option key={p} value={p}>
                      {providerLabel(p, lang)}
                    </option>
                  ))}
                </select>
              </label>
              {local.ai.provider === "rules" && <div className="callout wide">{t("settings.ai.rulesInfo").replace("{column}", "{{column}}")}</div>}
              {local.ai.provider !== "rules" && (
                <>
                  <label className="field">
                    <span>{t("settings.ai.baseUrl")}</span>
                    <input type="url" value={local.ai.base_url} placeholder={defaults.base_url} onChange={(e) => setAi({ base_url: e.target.value })} spellCheck={false} />
                  </label>
                  <label className="field">
                    <span>{t("settings.ai.model")}</span>
                    <input type="text" value={local.ai.model} placeholder={defaults.model} onChange={(e) => setAi({ model: e.target.value })} spellCheck={false} />
                  </label>
                  {local.ai.provider !== "ollama" && (
                    <label className="field wide">
                      <span>
                        {t("settings.ai.apiKey")}
                        {local.ai.provider === "openai_compatible" ? t("settings.ai.apiKeyOptional") : ""}
                      </span>
                      <input type="password" value={local.ai.api_key} onChange={(e) => setAi({ api_key: e.target.value })} autoComplete="off" />
                    </label>
                  )}
                  <div className="check-list wide">
                    <label className="checkbox block">
                      <input type="checkbox" checked={local.ai.include_sample} onChange={(e) => setAi({ include_sample: e.target.checked })} />
                      <span>{t("settings.ai.includeSample")}</span>
                    </label>
                    <label className="checkbox block">
                      <input type="checkbox" checked={local.ai.fallback_to_rules} onChange={(e) => setAi({ fallback_to_rules: e.target.checked })} />
                      <span>{t("settings.ai.fallback")}</span>
                    </label>
                  </div>
                  <div className="inline-fields wide">
                    <label className="field inline">
                      <span>{t("settings.ai.sampleLines")}</span>
                      <input type="number" min={1} max={200} value={local.ai.sample_lines} onChange={(e) => setAi({ sample_lines: Math.max(1, Number(e.target.value) || 1) })} style={{ width: 80 }} />
                    </label>
                    <label className="field inline">
                      <span>{t("settings.ai.timeout")}</span>
                      <input type="number" min={5} max={600} value={local.ai.timeout_secs} onChange={(e) => setAi({ timeout_secs: Math.max(5, Number(e.target.value) || 5) })} style={{ width: 80 }} />
                    </label>
                  </div>
                  <div className="inline-fields wide">
                    <button className="btn" onClick={test} disabled={testing}>
                      {testing ? <span className="spinner" /> : null} {t("settings.ai.test")}
                    </button>
                    {testResult && <span className={`callout ${testResult.ok ? "good" : "bad"}`}>{testResult.message}</span>}
                  </div>
                  {local.ai.provider === "ollama" && (
                    <div className="muted small wide">
                      {ollamaParts.map((part, i) =>
                        part === "{pull}" ? <kbd key={i}>ollama pull llama3.1</kbd> : part === "{serve}" ? <kbd key={i}>ollama serve</kbd> : <span key={i}>{part}</span>,
                      )}
                    </div>
                  )}
                  {local.ai.provider === "openai_compatible" && <div className="muted small wide">{t("settings.ai.openaiHint")}</div>}
                  {local.ai.provider === "anthropic" && <div className="muted small wide">{t("settings.ai.anthropicHint")}</div>}
                </>
              )}
            </div>
          </fieldset>

          <fieldset>
            <legend>{t("settings.behaviour.legend")}</legend>
            <div className="form-grid">
              <label className="field">
                <span>{t("settings.language")}</span>
                <select value={local.language ?? "system"} onChange={(e) => setLocal({ ...local, language: e.target.value })}>
                  <option value="system">{t("settings.language.system")}</option>
                  <option value="en">{t("settings.language.en")}</option>
                  <option value="tr">{t("settings.language.tr")}</option>
                </select>
              </label>
              <label className="field">
                <span>{t("settings.theme")}</span>
                <select value={local.theme} onChange={(e) => setLocal({ ...local, theme: e.target.value })}>
                  <option value="system">{t("settings.theme.system")}</option>
                  <option value="light">{t("settings.theme.light")}</option>
                  <option value="dark">{t("settings.theme.dark")}</option>
                </select>
              </label>
            </div>
            <div className="check-list">
              <label className="checkbox block">
                <input type="checkbox" checked={local.watch_clipboard} onChange={(e) => setLocal({ ...local, watch_clipboard: e.target.checked })} />
                <span>{t("settings.watchClipboard")}</span>
              </label>
              <label className="checkbox block">
                <input type="checkbox" checked={local.auto_paste_on_focus} onChange={(e) => setLocal({ ...local, auto_paste_on_focus: e.target.checked })} />
                <span>{t("settings.autoPaste")}</span>
              </label>
              <label className="checkbox block">
                <input type="checkbox" checked={local.auto_copy_result} onChange={(e) => setLocal({ ...local, auto_copy_result: e.target.checked })} />
                <span>{t("settings.autoCopy")}</span>
              </label>
            </div>
          </fieldset>

          {paths && (
            <fieldset>
              <legend>{t("settings.data.legend")}</legend>
              <div className="small muted paths">
                <div>
                  {t("settings.data.recipes")} <span className="mono">{paths.recipes_dir}</span>
                </div>
                <div>
                  {t("settings.data.settings")} <span className="mono">{paths.settings_file}</span>
                </div>
              </div>
            </fieldset>
          )}
        </div>
        <div className="modal-footer">
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button className="btn primary" onClick={() => onSave(local)}>
            {t("settings.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
