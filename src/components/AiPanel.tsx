import { useState } from "react";
import { api, writeClipboard } from "../bridge";
import { useI18n } from "../i18n";
import type { AppSettings, GeneratedRecipe, Recipe } from "../types";
import { formatLabel, providerLabel, recipeName, reportBadge, stepSummary } from "../utils";

interface Props {
  input: string;
  settings: AppSettings | null;
  activeRecipe: Recipe | null;
  onApply: (generated: GeneratedRecipe) => void;
  onSaved: () => Promise<void>;
  onOpenSettings: () => void;
  onToast: (message: string) => void;
}

const EXAMPLE_KEYS = ["ai.example.1", "ai.example.2", "ai.example.3", "ai.example.4", "ai.example.5", "ai.example.6"];

export default function AiPanel({ input, settings, activeRecipe, onApply, onSaved, onOpenSettings, onToast }: Props) {
  const { lang, t } = useI18n();
  const [instruction, setInstruction] = useState("");
  const [refine, setRefine] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [result, setResult] = useState<GeneratedRecipe | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saveName, setSaveName] = useState("");

  const provider = settings?.ai.provider ?? "rules";
  const providerName = providerLabel(provider, lang);
  const model = settings?.ai.model || (provider === "ollama" ? "llama3.1" : provider === "anthropic" ? "claude-opus-5-5" : provider === "openai_compatible" ? "local-model" : "");
  const examples = EXAMPLE_KEYS.map((k) => t(k));

  const generate = async () => {
    if (!instruction.trim()) return;
    setGenerating(true);
    setError(null);
    setResult(null);
    try {
      const g = await api.aiGenerate(instruction, input, refine && activeRecipe ? activeRecipe : undefined, lang);
      setResult(g);
      setSaveName(g.recipe.name);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setGenerating(false);
    }
  };

  const save = async () => {
    if (!result) return;
    try {
      const saved = await api.saveRecipe({ ...result.recipe, id: "", name: saveName.trim() || result.recipe.name }, lang);
      await onSaved();
      onToast(t("ai.toast.saved", { name: recipeName(saved, lang) }));
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    }
  };

  const copyJson = async () => {
    if (!result) return;
    await writeClipboard(JSON.stringify(result.recipe, null, 2));
    onToast(t("recipes.toast.jsonCopied"));
  };

  const badge = result?.test_report ? reportBadge(result.test_report, lang) : null;
  const offlineNote = t("ai.offlineNote");
  const offlineWord = t("ai.offlineMode");
  const offlineIdx = offlineNote.indexOf(offlineWord);

  return (
    <div className="split">
      <div className="pane">
        <div className="pane-header">
          <h2>{t("ai.title")}</h2>
          <div className="spacer" />
          <span className="badge" title={t("ai.change.title")}>
            {providerName}
            {model ? (
              <span className="muted" style={{ fontWeight: 400 }}>
                · {model}
              </span>
            ) : null}
          </span>
          <button className="btn small ghost" onClick={onOpenSettings}>
            {t("ai.change")}
          </button>
        </div>
        <div className="detail">
          <p className="muted small" style={{ margin: 0 }}>
            {t("ai.intro")}
            {provider === "rules" &&
              (offlineIdx >= 0 ? (
                <>
                  {offlineNote.slice(0, offlineIdx)}
                  <b>{offlineWord}</b>
                  {offlineNote.slice(offlineIdx + offlineWord.length)}
                </>
              ) : (
                offlineNote
              ))}
          </p>
          <textarea
            className="box"
            placeholder={examples[0]}
            value={instruction}
            onChange={(e) => setInstruction(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) generate();
            }}
          />
          <div className="chips">
            {examples.map((ex) => (
              <button key={ex} className="chip" onClick={() => setInstruction(ex)}>
                {ex}
              </button>
            ))}
          </div>
          <div className="actions" style={{ alignItems: "center" }}>
            <button className="btn primary" onClick={generate} disabled={generating || !instruction.trim()}>
              {generating ? <span className="spinner" /> : null}
              {generating ? t("ai.generating") : t("ai.generate")}
            </button>
            {activeRecipe && (
              <label className="checkbox" title={t("ai.refine.title")}>
                <input type="checkbox" checked={refine} onChange={(e) => setRefine(e.target.checked)} />
                {t("ai.refine", { name: recipeName(activeRecipe, lang) })}
              </label>
            )}
            <span className="muted small">{t("ai.shortcut")}</span>
          </div>
          {!input.trim() && <div className="callout warn">{t("ai.noInput")}</div>}
          {settings && provider !== "rules" && settings.ai.include_sample && (
            <div className="muted small">{t("ai.sampleNote", { n: settings.ai.sample_lines, provider: providerName })}</div>
          )}
        </div>
      </div>

      <div className="pane">
        <div className="pane-header">
          <h2>{t("ai.result")}</h2>
          {result && (
            <span className="badge accent">
              {providerLabel(result.provider, lang)}
              {result.attempts > 1 ? t("ai.attempts", { n: result.attempts }) : ""}
            </span>
          )}
        </div>
        <div className="detail">
          {error && <div className="callout bad">{error}</div>}
          {!result && !error && (
            <div className="empty-state">
              <span className="big">{t("ai.emptyTitle")}</span>
              <span>{t("ai.emptyBody")}</span>
            </div>
          )}
          {result && (
            <>
              {result.used_fallback && <div className="callout warn">{t("ai.fallback", { reason: result.fallback_reason ?? "" })}</div>}
              <div>
                <h3>{recipeName(result.recipe, lang)}</h3>
                {result.explanation && <p style={{ margin: "4px 0 0" }}>{result.explanation}</p>}
              </div>
              <div className="actions">
                <button className="btn primary" onClick={() => onApply(result)}>
                  {t("common.useInConverter")}
                </button>
                <span className="inline-form">
                  <input type="text" value={saveName} onChange={(e) => setSaveName(e.target.value)} placeholder={t("common.recipeName")} style={{ width: 220 }} />
                  <button className="btn" onClick={save}>
                    {t("common.saveAsRecipe")}
                  </button>
                </span>
                <button className="btn" onClick={copyJson}>
                  {t("common.copyJson")}
                </button>
              </div>
              <div className="card">
                <h4>{t("common.steps")}</h4>
                <div className="small">
                  {t("recipes.inputLabel")} <b>{result.recipe.input.format ? formatLabel(result.recipe.input.format, lang) : t("recipes.autoDetect")}</b> · {t("recipes.outputLabel")}{" "}
                  <b>{formatLabel(result.recipe.output.format, lang)}</b>
                </div>
                {result.recipe.steps.length ? (
                  <ol className="steps-list">
                    {result.recipe.steps.map((s, i) => (
                      <li key={i}>{stepSummary(s, lang)}</li>
                    ))}
                  </ol>
                ) : (
                  <span className="muted small">{t("common.noSteps")}</span>
                )}
              </div>
              {result.test_output !== undefined && (
                <div className="card">
                  <h4>{t("ai.testRun")}</h4>
                  {badge && result.test_report && (
                    <div className="report-head">
                      <span className={`badge ${badge.tone === "good" ? "good" : badge.tone === "warn" ? "warn" : badge.tone === "bad" ? "bad" : ""}`}>
                        <span className="dot" />
                        {badge.label}
                      </span>
                      {result.test_report.stats.input_rows != null && (
                        <span className="muted small">{t("common.rows", { a: result.test_report.stats.input_rows, b: result.test_report.stats.output_rows ?? "?" })}</span>
                      )}
                    </div>
                  )}
                  <pre>
                    {result.test_output.slice(0, 4000)}
                    {result.test_output.length > 4000 ? "\n…" : ""}
                  </pre>
                  {result.test_report && result.test_report.issues.filter((i) => i.severity !== "info").length > 0 && (
                    <ul className="issues">
                      {result.test_report.issues
                        .filter((i) => i.severity !== "info")
                        .slice(0, 5)
                        .map((issue, i) => (
                          <li key={i} className={`issue ${issue.severity}`}>
                            <span className="sev">{t(`severity.${issue.severity}`)}</span>
                            <span>{issue.message}</span>
                          </li>
                        ))}
                    </ul>
                  )}
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
