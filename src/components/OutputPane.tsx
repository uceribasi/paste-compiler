import { useState } from "react";
import { saveTextToFile } from "../bridge";
import { useI18n } from "../i18n";
import type { ConvertResult, FormatInfo, OutputOptions } from "../types";
import { categoryLabel, fileNameFor, formatBytes, formatLabel } from "../utils";
import OptionsForm from "./OptionsForm";

interface Props {
  result: ConvertResult | null;
  error: string | null;
  busy: boolean;
  outputs: FormatInfo[];
  suggestions: FormatInfo[];
  outputFormat: string;
  onFormat: (id: string) => void;
  options: OutputOptions;
  onOptions: (options: OutputOptions) => void;
  onCopy: () => void;
  onToast: (message: string) => void;
}

const SUGGESTED_COUNT = 7;

export default function OutputPane({ result, error, busy, outputs, suggestions, outputFormat, onFormat, options, onOptions, onCopy, onToast }: Props) {
  const { lang, t } = useI18n();
  const [showAll, setShowAll] = useState(false);
  const current = outputs.find((f) => f.id === outputFormat);
  const chips = (suggestions.length ? suggestions : outputs).slice(0, SUGGESTED_COUNT);
  const inChips = chips.some((c) => c.id === outputFormat);
  const label = (f: FormatInfo) => formatLabel(f.id, lang, f.label);

  const save = async () => {
    if (!result) return;
    try {
      const path = await saveTextToFile(fileNameFor(result.output_format, current?.extension ?? "txt"), result.output, current?.extension ?? "txt");
      if (path) onToast(t("output.savedTo", { path }));
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <>
      <div className="pane-header">
        <h2>{t("common.output")}</h2>
        {current && <span className="badge accent">{label(current)}</span>}
        {result && <span className="muted small">{formatBytes(result.report.stats.output_bytes)}</span>}
        <div className="spacer" />
        <button className="btn small" onClick={save} disabled={!result}>
          {t("output.saveAsFile")}
        </button>
        <button className="btn small primary" onClick={onCopy} disabled={!result} title="⌘↩">
          {t("output.copy")}
        </button>
      </div>
      <div className="toolbar">
        <div className="chips">
          {chips.map((f) => (
            <button key={f.id} className={`chip ${f.id === outputFormat ? "active" : ""}`} onClick={() => onFormat(f.id)} title={categoryLabel(f.category, lang)}>
              {label(f)}
            </button>
          ))}
          {!inChips && current && (
            <button className="chip active" onClick={() => setShowAll(true)}>
              {label(current)}
            </button>
          )}
          <button className="chip" onClick={() => setShowAll((v) => !v)}>
            {showAll ? t("output.fewer") : t("output.allFormats")}
          </button>
        </div>
        {showAll && (
          <select value={outputFormat} onChange={(e) => onFormat(e.target.value)}>
            {["tabular", "tree", "list", "text"].map((cat) => (
              <optgroup key={cat} label={categoryLabel(cat, lang)}>
                {outputs
                  .filter((f) => f.category === cat)
                  .map((f) => (
                    <option key={f.id} value={f.id}>
                      {label(f)}
                    </option>
                  ))}
              </optgroup>
            ))}
          </select>
        )}
      </div>
      <OptionsForm formatId={outputFormat} options={options} onChange={onOptions} />
      <div className="pane-body">
        {error ? (
          <pre className="editor empty" style={{ color: "var(--bad)" }}>
            {error}
          </pre>
        ) : result ? (
          <pre className="editor" aria-busy={busy}>
            {result.output || <span className="muted">{t("output.emptyOutput")}</span>}
          </pre>
        ) : (
          <pre className="editor empty">{busy ? t("output.converting") : t("output.empty")}</pre>
        )}
      </div>
    </>
  );
}
