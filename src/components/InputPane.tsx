import { useState } from "react";
import { useI18n } from "../i18n";
import type { Analysis, FormatInfo, ParseOptions } from "../types";
import { confidenceLabel, delimiterLabel, formatLabel } from "../utils";

interface Props {
  input: string;
  onInput: (value: string) => void;
  onClear: () => void;
  onPaste: () => void;
  analysis: Analysis | null;
  inputFormat: string;
  onInputFormat: (id: string) => void;
  inputFormats: FormatInfo[];
  parse: ParseOptions;
  onParse: (parse: ParseOptions) => void;
  busy: boolean;
}

export default function InputPane({ input, onInput, onClear, onPaste, analysis, inputFormat, onInputFormat, inputFormats, parse, onParse, busy }: Props) {
  const { lang, t } = useI18n();
  const [showPreview, setShowPreview] = useState(false);
  const detection = analysis?.detection;
  const isDelimited = inputFormat === "auto" ? detection?.format === "csv" || detection?.format === "tsv" : inputFormat === "csv" || inputFormat === "tsv";
  const preview = analysis?.preview;
  const headerValue = parse.has_header === undefined ? "auto" : parse.has_header ? "yes" : "no";
  const detectedLabel = detection ? formatLabel(detection.format, lang, inputFormats.find((f) => f.id === detection.format)?.label) : null;

  const summaryText = (a: Analysis): string => {
    const s = a.summary;
    switch (s.kind) {
      case "table":
        return t("input.summary.table", { rows: s.rows ?? 0, cols: s.columns ?? 0 }) + (s.has_header === false ? t("input.summary.noHeader") : "");
      case "tree": {
        const kind = s.tree_kind === "array" ? t("input.summary.array") : s.tree_kind === "object" ? t("input.summary.object") : t("input.summary.value");
        const unit = s.tree_kind === "array" ? t("input.summary.items") : t("input.summary.keys");
        return `${kind}${s.items != null ? ` · ${s.items} ${unit}` : ""}`;
      }
      case "list":
        return t("input.summary.list", { n: s.items ?? 0 });
      case "text":
        return t("input.summary.text", { n: s.items ?? 0 });
    }
  };

  return (
    <>
      <div className="pane-header">
        <h2>{t("common.input")}</h2>
        {detection && detectedLabel && (
          <span
            className={`badge ${confidenceLabel(detection.confidence) === "high" ? "good" : confidenceLabel(detection.confidence) === "medium" ? "warn" : ""}`}
            title={detection.reasons.join("\n")}
          >
            <span className="dot" />
            {detectedLabel}
            {detection.delimiter && detection.format === "csv" ? ` (${delimiterLabel(detection.delimiter, lang)})` : ""}
            <span className="muted" style={{ fontWeight: 400 }}>
              {Math.round(detection.confidence * 100)}%
            </span>
          </span>
        )}
        {analysis && <span className="muted small">{summaryText(analysis)}</span>}
        {busy && <span className="spinner" aria-label={t("common.working")} />}
        <div className="spacer" />
        {preview && (
          <button className="btn small ghost" onClick={() => setShowPreview((v) => !v)}>
            {showPreview ? t("input.hideTable") : t("input.showTable")}
          </button>
        )}
        <button className="btn small" onClick={onPaste}>
          {t("input.paste")}
        </button>
        <button className="btn small" onClick={onClear} disabled={!input}>
          {t("input.clear")}
        </button>
      </div>
      <div className="toolbar">
        <label className="field inline">
          <span>{t("input.readAs")}</span>
          <select value={inputFormat} onChange={(e) => onInputFormat(e.target.value)}>
            <option value="auto">
              {t("common.autoDetect")}
              {detectedLabel ? ` (${detectedLabel})` : ""}
            </option>
            {inputFormats.map((f) => (
              <option key={f.id} value={f.id}>
                {formatLabel(f.id, lang, f.label)}
              </option>
            ))}
          </select>
        </label>
        {isDelimited && (
          <>
            <label className="field inline">
              <span>{t("input.delimiter")}</span>
              <select value={parse.delimiter ?? ""} onChange={(e) => onParse({ ...parse, delimiter: e.target.value || undefined })}>
                <option value="">
                  {t("common.auto")}
                  {detection?.delimiter ? ` (${delimiterLabel(detection.delimiter, lang)})` : ""}
                </option>
                <option value=",">{t("input.delim.comma")}</option>
                <option value=";">{t("input.delim.semicolon")}</option>
                <option value={"\t"}>{t("input.delim.tab")}</option>
                <option value="|">{t("input.delim.pipe")}</option>
              </select>
            </label>
            <label className="field inline">
              <span>{t("input.headerRow")}</span>
              <select
                value={headerValue}
                onChange={(e) => {
                  const v = e.target.value;
                  onParse({ ...parse, has_header: v === "auto" ? undefined : v === "yes" });
                }}
              >
                <option value="auto">
                  {t("common.auto")}
                  {detection?.has_header !== undefined ? ` (${detection.has_header ? t("common.yes").toLowerCase() : t("common.no").toLowerCase()})` : ""}
                </option>
                <option value="yes">{t("common.yes")}</option>
                <option value="no">{t("common.no")}</option>
              </select>
            </label>
          </>
        )}
        <label className="checkbox" title={t("input.inferTypes.title")}>
          <input type="checkbox" checked={parse.infer_types !== false} onChange={(e) => onParse({ ...parse, infer_types: e.target.checked ? undefined : false })} />
          {t("input.inferTypes")}
        </label>
      </div>
      <div className="pane-body">
        <textarea className="editor" value={input} onChange={(e) => onInput(e.target.value)} placeholder={t("input.placeholder")} spellCheck={false} />
        {showPreview && preview && (
          <div className="preview">
            <table>
              <thead>
                <tr>
                  {preview.headers.map((h, i) => (
                    <th key={i}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {preview.rows.map((row, ri) => (
                  <tr key={ri}>
                    {row.map((cell, ci) => (
                      <td key={ci} title={cell}>
                        {cell}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
            {preview.total_rows > preview.rows.length && (
              <div className="muted small" style={{ padding: "4px 8px" }}>
                {t("input.moreRows", { n: preview.total_rows - preview.rows.length })}
              </div>
            )}
          </div>
        )}
        {analysis && analysis.issues.length > 0 && (
          <ul className="issues" style={{ padding: "6px 10px", borderTop: "1px solid var(--border)" }}>
            {analysis.issues.slice(0, 4).map((issue, i) => (
              <li key={i} className={`issue ${issue.severity}`}>
                <span className="sev">{t(`severity.${issue.severity}`)}</span>
                <span>{issue.message}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </>
  );
}
