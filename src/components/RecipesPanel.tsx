import { useEffect, useMemo, useState } from "react";
import { api, confirmDialog, pickRecipeFile, pickSavePath, writeClipboard } from "../bridge";
import { useI18n } from "../i18n";
import type { ConvertResult, Recipe } from "../types";
import { formatLabel, recipeDescription, recipeName, reportBadge, shortDate, stepSummary } from "../utils";

interface Props {
  recipes: Recipe[];
  input: string;
  onApply: (recipe: Recipe) => void;
  onRefresh: () => Promise<void>;
  onToast: (message: string) => void;
}

export default function RecipesPanel({ recipes, input, onApply, onRefresh, onToast }: Props) {
  const { lang, t } = useI18n();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [preview, setPreview] = useState<ConvertResult | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [editorText, setEditorText] = useState("");
  const [editorError, setEditorError] = useState<string | null>(null);
  const [share, setShare] = useState<string | null>(null);
  const [importText, setImportText] = useState("");
  const [importing, setImporting] = useState(false);

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return recipes;
    return recipes.filter((r) =>
      [r.name, r.description, recipeName(r, lang), recipeDescription(r, lang), r.tags.join(" "), r.output.format].join(" ").toLowerCase().includes(q),
    );
  }, [recipes, filter, lang]);

  const selected = recipes.find((r) => r.id === selectedId) ?? null;

  useEffect(() => {
    if (!selected && filtered.length) setSelectedId(filtered[0].id);
  }, [filtered, selected]);

  useEffect(() => {
    setPreview(null);
    setPreviewError(null);
    setEditing(false);
    setShare(null);
    if (!selected || !input.trim()) return;
    let cancelled = false;
    api
      .runRecipe(input, selected.id, undefined, lang)
      .then((r) => {
        if (!cancelled) setPreview(r);
      })
      .catch((e) => {
        if (!cancelled) setPreviewError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selected, input, lang]);

  const fail = (e: unknown) => onToast(e instanceof Error ? e.message : String(e));
  const nameOf = (r: Recipe) => recipeName(r, lang);

  const startEdit = () => {
    if (!selected) return;
    const copy = { ...selected };
    delete (copy as Partial<Recipe>).builtin;
    setEditorText(JSON.stringify(copy, null, 2));
    setEditorError(null);
    setEditing(true);
  };

  const saveEdit = async () => {
    try {
      const parsed = await api.parseRecipe(editorText, lang);
      if (selected?.builtin) {
        parsed.id = "";
        if (parsed.name === selected.name) parsed.name = `${selected.name}${t("recipes.copySuffix")}`;
      }
      const saved = await api.saveRecipe(parsed, lang);
      await onRefresh();
      setSelectedId(saved.id);
      setEditing(false);
      onToast(t("recipes.toast.saved", { name: nameOf(saved) }));
    } catch (e) {
      setEditorError(e instanceof Error ? e.message : String(e));
    }
  };

  const duplicate = async () => {
    if (!selected) return;
    try {
      const saved = await api.saveRecipe({ ...selected, id: "", name: `${selected.name}${t("recipes.copySuffix")}`, builtin: false, translations: undefined }, lang);
      await onRefresh();
      setSelectedId(saved.id);
      onToast(t("recipes.toast.duplicated", { name: nameOf(saved) }));
    } catch (e) {
      fail(e);
    }
  };

  const remove = async () => {
    if (!selected || selected.builtin) return;
    if (!(await confirmDialog(t("recipes.confirmDelete", { name: nameOf(selected) })))) return;
    try {
      await api.deleteRecipe(selected.id);
      setSelectedId(null);
      await onRefresh();
      onToast(t("recipes.toast.deleted"));
    } catch (e) {
      fail(e);
    }
  };

  const showShare = async () => {
    if (!selected) return;
    try {
      const exported = await api.exportRecipe(selected.id);
      setShare(exported.share);
    } catch (e) {
      fail(e);
    }
  };

  const copyShare = async () => {
    if (!share) return;
    await writeClipboard(share);
    onToast(t("recipes.toast.shareCopied"));
  };

  const copyJson = async () => {
    if (!selected) return;
    try {
      const exported = await api.exportRecipe(selected.id);
      await writeClipboard(exported.json);
      onToast(t("recipes.toast.jsonCopied"));
    } catch (e) {
      fail(e);
    }
  };

  const exportFile = async () => {
    if (!selected) return;
    try {
      const path = await pickSavePath(`${selected.name.replace(/[^\w.-]+/g, "-").toLowerCase()}.recipe.json`);
      if (!path) return;
      await api.exportRecipeToFile(selected.id, path);
      onToast(t("recipes.toast.exported", { path }));
    } catch (e) {
      fail(e);
    }
  };

  const importFromText = async () => {
    if (!importText.trim()) return;
    try {
      const r = await api.importRecipe(importText, lang);
      await onRefresh();
      setSelectedId(r.id);
      setImportText("");
      setImporting(false);
      onToast(t("recipes.toast.imported", { name: nameOf(r) }));
    } catch (e) {
      fail(e);
    }
  };

  const importFromFile = async () => {
    try {
      const path = await pickRecipeFile();
      if (!path) return;
      const r = await api.importRecipeFromFile(path, lang);
      await onRefresh();
      setSelectedId(r.id);
      setImporting(false);
      onToast(t("recipes.toast.imported", { name: nameOf(r) }));
    } catch (e) {
      fail(e);
    }
  };

  const previewBadge = preview ? reportBadge(preview.report, lang) : null;
  const editorExample = '{"op": "filter_rows", "column": "age", "condition": "gt", "value": "30"}';
  const [hintBefore, hintAfter] = t("recipes.editorHint").split("{example}");

  return (
    <div className="split">
      <div className="pane">
        <div className="pane-header">
          <h2>{t("recipes.title")}</h2>
          <div className="spacer" />
          <button className="btn small" onClick={() => setImporting((v) => !v)}>
            {t("recipes.import")}
          </button>
        </div>
        <div className="toolbar">
          <input type="text" placeholder={t("recipes.search")} value={filter} onChange={(e) => setFilter(e.target.value)} style={{ flex: 1 }} />
        </div>
        {importing && (
          <div className="card" style={{ margin: 10 }}>
            <h4>{t("recipes.importTitle")}</h4>
            <textarea className="box mono" style={{ minHeight: 80 }} placeholder={t("recipes.importPlaceholder")} value={importText} onChange={(e) => setImportText(e.target.value)} />
            <div className="actions">
              <button className="btn small primary" onClick={importFromText} disabled={!importText.trim()}>
                {t("recipes.importButton")}
              </button>
              <button className="btn small" onClick={importFromFile}>
                {t("recipes.chooseFile")}
              </button>
            </div>
          </div>
        )}
        <div className="list">
          {filtered.length === 0 && (
            <div className="empty-state">
              <span className="big">{t("recipes.emptyTitle")}</span>
              <span>{t("recipes.emptyBody")}</span>
            </div>
          )}
          {filtered.map((r) => (
            <div key={r.id} className={`list-item ${r.id === selectedId ? "active" : ""}`} onClick={() => setSelectedId(r.id)}>
              <div className="title">
                {nameOf(r)}
                {r.builtin && <span className="badge small">{t("common.builtin")}</span>}
                <span className="badge accent">{formatLabel(r.output.format, lang)}</span>
              </div>
              {recipeDescription(r, lang) && <div className="desc">{recipeDescription(r, lang)}</div>}
            </div>
          ))}
        </div>
      </div>

      <div className="pane">
        {!selected ? (
          <div className="empty-state">
            <span className="big">{t("recipes.select")}</span>
          </div>
        ) : editing ? (
          <>
            <div className="pane-header">
              <h2>{selected.builtin ? t("recipes.editCopyOf", { name: nameOf(selected) }) : t("recipes.edit", { name: nameOf(selected) })}</h2>
              <div className="spacer" />
              <button className="btn small" onClick={() => setEditing(false)}>
                {t("common.cancel")}
              </button>
              <button className="btn small primary" onClick={saveEdit}>
                {t("common.save")}
              </button>
            </div>
            <div className="detail">
              {editorError && <div className="callout bad">{editorError}</div>}
              <textarea className="box mono" style={{ minHeight: 420, flex: 1 }} value={editorText} onChange={(e) => setEditorText(e.target.value)} spellCheck={false} />
              <span className="muted small">
                {hintBefore}
                <code>{editorExample}</code>
                {hintAfter}
              </span>
            </div>
          </>
        ) : (
          <div className="detail">
            <div>
              <h3>{nameOf(selected)}</h3>
              <div className="muted small">
                {selected.builtin ? t("recipes.builtinRecipe") : `${t("recipes.savedRecipe")}${selected.updated_at ? t("recipes.updated", { date: shortDate(selected.updated_at, lang) }) : ""}`}
                {selected.author ? t("recipes.by", { author: selected.author }) : ""}
              </div>
            </div>
            {recipeDescription(selected, lang) && <p style={{ margin: 0 }}>{recipeDescription(selected, lang)}</p>}
            {selected.prompt && (
              <div className="callout">
                <b>{t("recipes.prompt")}</b> {selected.prompt}
              </div>
            )}
            <div className="actions">
              <button className="btn primary" onClick={() => onApply(selected)}>
                {t("common.useInConverter")}
              </button>
              <button className="btn" onClick={startEdit}>
                {selected.builtin ? t("recipes.editCopy") : t("recipes.editJson")}
              </button>
              <button className="btn" onClick={duplicate}>
                {t("recipes.duplicate")}
              </button>
              <button className="btn" onClick={showShare}>
                {t("recipes.shareCode")}
              </button>
              <button className="btn" onClick={copyJson}>
                {t("common.copyJson")}
              </button>
              <button className="btn" onClick={exportFile}>
                {t("recipes.exportFile")}
              </button>
              {!selected.builtin && (
                <button className="btn danger" onClick={remove}>
                  {t("recipes.delete")}
                </button>
              )}
            </div>
            {share && (
              <div className="card">
                <h4>{t("recipes.shareCode")}</h4>
                <pre style={{ whiteSpace: "pre-wrap", wordBreak: "break-all" }}>{share}</pre>
                <div className="actions">
                  <button className="btn small primary" onClick={copyShare}>
                    {t("recipes.copyShare")}
                  </button>
                  <span className="muted small">{t("recipes.shareHint")}</span>
                </div>
              </div>
            )}
            <div className="card">
              <h4>{t("recipes.whatItDoes")}</h4>
              <div className="small">
                {t("recipes.inputLabel")} <b>{selected.input.format ? formatLabel(selected.input.format, lang) : t("recipes.autoDetect")}</b>
                {selected.input.delimiter ? t("recipes.delimiterNote", { d: selected.input.delimiter === "\t" ? t("delim.tab") : selected.input.delimiter }) : ""}
                {" · "}
                {t("recipes.outputLabel")} <b>{formatLabel(selected.output.format, lang)}</b>
              </div>
              {selected.steps.length ? (
                <ol className="steps-list">
                  {selected.steps.map((s, i) => (
                    <li key={i}>{stepSummary(s, lang)}</li>
                  ))}
                </ol>
              ) : (
                <span className="muted small">{t("common.noSteps")}</span>
              )}
              {Object.entries(selected.output).filter(([k]) => k !== "format").length > 0 && (
                <div className="small muted">
                  {t("recipes.options")}{" "}
                  {Object.entries(selected.output)
                    .filter(([k, v]) => k !== "format" && v !== undefined && v !== null && v !== "")
                    .map(([k, v]) => `${k}=${JSON.stringify(v)}`)
                    .join(", ")}
                </div>
              )}
            </div>
            <div className="card">
              <h4>{t("recipes.preview")}</h4>
              {!input.trim() ? (
                <span className="muted small">{t("recipes.previewEmpty")}</span>
              ) : previewError ? (
                <div className="callout bad">{previewError}</div>
              ) : preview && previewBadge ? (
                <>
                  <div className="report-head">
                    <span className={`badge ${previewBadge.tone === "good" ? "good" : previewBadge.tone === "warn" ? "warn" : previewBadge.tone === "bad" ? "bad" : ""}`}>
                      <span className="dot" />
                      {previewBadge.label}
                    </span>
                    <span className="muted small">
                      {preview.report.stats.input_rows != null ? t("common.rows", { a: preview.report.stats.input_rows, b: preview.report.stats.output_rows ?? "?" }) : ""}
                    </span>
                  </div>
                  <pre>
                    {preview.output.slice(0, 4000)}
                    {preview.output.length > 4000 ? "\n…" : ""}
                  </pre>
                </>
              ) : (
                <span className="spinner" />
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
