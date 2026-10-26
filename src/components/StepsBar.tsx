import { useState } from "react";
import { useI18n } from "../i18n";
import type { Recipe, Step } from "../types";
import { recipeDescription, recipeName, stepSummary } from "../utils";

interface Props {
  steps: Step[];
  activeRecipe: Recipe | null;
  onRemove: (index: number) => void;
  onClear: () => void;
  onSaveAs: (name: string) => void;
  onUpdate: () => void;
  onDescribe: () => void;
}

export default function StepsBar({ steps, activeRecipe, onRemove, onClear, onSaveAs, onUpdate, onDescribe }: Props) {
  const { lang, t } = useI18n();
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");

  const submit = () => {
    if (!name.trim()) return;
    onSaveAs(name.trim());
    setNaming(false);
    setName("");
  };

  return (
    <div className="steps-bar">
      {activeRecipe ? (
        <span className="badge accent" title={recipeDescription(activeRecipe, lang)}>
          {t("steps.recipe", { name: recipeName(activeRecipe, lang) })}
          {activeRecipe.builtin ? t("steps.builtinSuffix") : ""}
        </span>
      ) : (
        <span className="label">{t("common.steps")}</span>
      )}
      {steps.length === 0 && <span className="muted small">{t("steps.none")}</span>}
      <div className="chips">
        {steps.map((step, i) => (
          <span key={i} className="chip removable" title={JSON.stringify(step)}>
            {stepSummary(step, lang)}
            <button onClick={() => onRemove(i)} aria-label={t("steps.remove")} title={t("steps.remove")}>
              ×
            </button>
          </span>
        ))}
      </div>
      <div className="spacer" style={{ flex: 1 }} />
      {steps.length > 0 && (
        <button className="btn small ghost" onClick={onClear}>
          {t("steps.clear")}
        </button>
      )}
      <button className="btn small" onClick={onDescribe}>
        {t("steps.describe")}
      </button>
      {activeRecipe && !activeRecipe.builtin && (
        <button className="btn small" onClick={onUpdate} title={t("steps.update.title")}>
          {t("steps.update")}
        </button>
      )}
      {naming ? (
        <span className="inline-form">
          <input
            type="text"
            autoFocus
            placeholder={t("common.recipeName")}
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
              if (e.key === "Escape") setNaming(false);
            }}
          />
          <button className="btn small primary" onClick={submit} disabled={!name.trim()}>
            {t("common.save")}
          </button>
          <button className="btn small ghost" onClick={() => setNaming(false)}>
            {t("common.cancel")}
          </button>
        </span>
      ) : (
        <button className="btn small" onClick={() => setNaming(true)} title={t("steps.saveAs.title")}>
          {t("steps.saveAs")}
        </button>
      )}
    </div>
  );
}
