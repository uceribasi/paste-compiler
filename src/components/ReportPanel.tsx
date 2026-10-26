import { useState } from "react";
import { useI18n } from "../i18n";
import type { ConvertResult } from "../types";
import { formatBytes, reportBadge } from "../utils";

interface Props {
  result: ConvertResult | null;
}

export default function ReportPanel({ result }: Props) {
  const { lang, t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const report = result?.report;
  const badge = reportBadge(report, lang);
  const issues = report?.issues ?? [];
  const warningsAndErrors = issues.filter((i) => i.severity !== "info");
  const infos = issues.filter((i) => i.severity === "info");
  const shown = expanded ? issues : warningsAndErrors.slice(0, 4);

  return (
    <div className="report">
      <div className="report-head">
        <span className={`badge ${badge.tone === "good" ? "good" : badge.tone === "warn" ? "warn" : badge.tone === "bad" ? "bad" : ""}`}>
          <span className="dot" />
          {badge.label}
        </span>
        {report && <span className="muted small">{t(`report.roundTrip.${report.round_trip}`)}</span>}
        <div className="spacer" style={{ flex: 1 }} />
        {report && (issues.length > shown.length || expanded) && (
          <button className="btn small ghost" onClick={() => setExpanded((v) => !v)}>
            {expanded ? t("common.less") : t("common.details", { n: issues.length })}
          </button>
        )}
      </div>
      {report && (
        <div className="stats">
          {report.stats.input_rows != null && (
            <span>
              {t("report.rows")} <b>{report.stats.input_rows}</b> → <b>{report.stats.output_rows ?? "?"}</b>
            </span>
          )}
          {report.stats.input_columns != null && (
            <span>
              {t("report.columns")} <b>{report.stats.input_columns}</b>
              {report.stats.output_columns != null && (
                <>
                  {" "}
                  → <b>{report.stats.output_columns}</b>
                </>
              )}
            </span>
          )}
          <span>
            {t("report.size")} <b>{formatBytes(report.stats.input_bytes)}</b> → <b>{formatBytes(report.stats.output_bytes)}</b>
          </span>
          <span>
            {t("report.time")} <b>{report.stats.elapsed_ms} ms</b>
          </span>
          {infos.length > 0 && !expanded && <span>{t("report.notes", { n: infos.length })}</span>}
        </div>
      )}
      {shown.length > 0 && (
        <ul className="issues">
          {shown.map((issue, i) => (
            <li key={i} className={`issue ${issue.severity}`}>
              <span className="sev">{t(`severity.${issue.severity}`)}</span>
              <span>{issue.message}</span>
            </li>
          ))}
        </ul>
      )}
      {report && report.differences.length > 0 && (
        <details open={expanded}>
          <summary className="small muted" style={{ cursor: "pointer" }}>
            {t("report.differences", { n: report.differences.length })}
          </summary>
          <ul className="issues" style={{ marginTop: 6 }}>
            {report.differences.map((d, i) => (
              <li key={i} className="issue warning">
                <span className="sev">{t("severity.diff")}</span>
                <span className="mono">{d}</span>
              </li>
            ))}
          </ul>
        </details>
      )}
      {!report && <span className="muted small">{t("report.empty")}</span>}
    </div>
  );
}
