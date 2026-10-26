// Pure helpers shared by the UI (unit-tested with vitest).

import { translate, type Lang } from "./i18n";
import type { OutputOptions, ParseOptions, Recipe, Step, ValidationReport } from "./types";

export const PROVIDER_DEFAULTS: Record<string, { base_url: string; model: string }> = {
  rules: { base_url: "", model: "" },
  ollama: { base_url: "http://localhost:11434", model: "llama3.1" },
  anthropic: { base_url: "https://api.anthropic.com", model: "claude-opus-5-5" },
  openai_compatible: { base_url: "http://localhost:1234/v1", model: "local-model" },
};

export function providerLabel(provider: string, lang: Lang): string {
  const key = `provider.${provider}`;
  const label = translate(lang, key);
  return label === key ? provider : label;
}

/** Localized label for an input or output format id; falls back to `fallback` or the id. */
export function formatLabel(id: string, lang: Lang, fallback?: string): string {
  const key = `fmt.${id}`;
  const label = translate(lang, key);
  if (label !== key) return label;
  return fallback ?? id.replace(/_/g, " ");
}

export function categoryLabel(category: string, lang: Lang): string {
  const key = `cat.${category}`;
  const label = translate(lang, key);
  return label === key ? category : label;
}

export function recipeName(recipe: Pick<Recipe, "name" | "translations">, lang: Lang): string {
  return recipe.translations?.[lang]?.name || recipe.name;
}

export function recipeDescription(recipe: Pick<Recipe, "description" | "translations">, lang: Lang): string {
  return recipe.translations?.[lang]?.description || recipe.description;
}

export function confidenceLabel(confidence: number): "high" | "medium" | "low" {
  if (confidence >= 0.85) return "high";
  if (confidence >= 0.6) return "medium";
  return "low";
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function fileNameFor(formatId: string, extension: string): string {
  const base = formatId.replace(/_/g, "-");
  return `pastecompiler-${base}.${extension || "txt"}`;
}

export type OptionField = {
  key: keyof OutputOptions;
  label: string;
  type: "text" | "number" | "boolean" | "select";
  placeholder?: string;
  choices?: string[];
  help?: string;
  /** Effective value when the option is not set (what the engine does by default). */
  defaultValue?: boolean;
};

type FieldSpec = Omit<OptionField, "label" | "help" | "placeholder"> & { labelKey: string; helpKey?: string; placeholderKey?: string; placeholder?: string };

const CSV_FIELDS: FieldSpec[] = [
  { key: "delimiter", labelKey: "opt.delimiter", type: "select", choices: [",", ";", "|", "\t"], helpKey: "opt.delimiter.help" },
  { key: "header", labelKey: "opt.header", type: "boolean", defaultValue: true },
  { key: "quote_all", labelKey: "opt.quoteAll", type: "boolean" },
  { key: "crlf", labelKey: "opt.crlf", type: "boolean" },
  { key: "null_as", labelKey: "opt.nullAs", type: "text", placeholderKey: "opt.nullAs.placeholder" },
];

function fieldSpecsFor(formatId: string): FieldSpec[] {
  switch (formatId) {
    case "json":
      return [{ key: "indent", labelKey: "opt.indent", type: "number", placeholder: "2" }];
    case "csv":
      return CSV_FIELDS;
    case "tsv":
      return [{ key: "header", labelKey: "opt.header", type: "boolean", defaultValue: true }];
    case "markdown_table":
      return [
        { key: "align", labelKey: "opt.align", type: "boolean", defaultValue: true },
        { key: "null_as", labelKey: "opt.nullAs", type: "text", placeholderKey: "opt.nullAs.placeholder" },
      ];
    case "html_table":
      return [{ key: "indent", labelKey: "opt.indent", type: "number", placeholder: "2" }];
    case "toml":
      return [{ key: "item_name", labelKey: "opt.itemName", type: "text", placeholder: "item" }];
    case "xml":
      return [
        { key: "root_name", labelKey: "opt.rootName", type: "text", placeholder: "root" },
        { key: "item_name", labelKey: "opt.itemElement", type: "text", placeholder: "item" },
        { key: "indent", labelKey: "opt.indent", type: "number", placeholder: "2" },
        { key: "declaration", labelKey: "opt.declaration", type: "boolean", defaultValue: true },
      ];
    case "sql_insert":
      return [
        { key: "table_name", labelKey: "opt.tableName", type: "text", placeholder: "data" },
        { key: "dialect", labelKey: "opt.dialect", type: "select", choices: ["postgres", "mysql", "sqlite"] },
        { key: "create_table", labelKey: "opt.createTable", type: "boolean" },
        { key: "batch", labelKey: "opt.batch", type: "boolean", defaultValue: true },
      ];
    case "markdown_list":
      return [{ key: "bullet", labelKey: "opt.bullet", type: "select", choices: ["-", "*", "+"] }];
    case "key_value":
      return [{ key: "separator", labelKey: "opt.separator", type: "select", choices: [": ", "=", " = ", ":"] }];
    case "template":
      return [
        { key: "template", labelKey: "opt.template", type: "text", placeholder: "{{name}} <{{email}}>", helpKey: "opt.template.help" },
        { key: "separator", labelKey: "opt.rowSeparator", type: "text", placeholder: "\\n" },
        { key: "prefix", labelKey: "opt.prefix", type: "text" },
        { key: "suffix", labelKey: "opt.suffix", type: "text" },
      ];
    default:
      return [];
  }
}

export function optionFieldsFor(formatId: string, lang: Lang = "en"): OptionField[] {
  return fieldSpecsFor(formatId).map(({ labelKey, helpKey, placeholderKey, placeholder, ...rest }) => ({
    ...rest,
    label: translate(lang, labelKey),
    help: helpKey ? translate(lang, helpKey).replace("{column}", "{{column}}").replace("{name | upper}", "{{name | upper}}") : undefined,
    placeholder: placeholderKey ? translate(lang, placeholderKey) : placeholder,
  }));
}

export function defaultOptionsFor(formatId: string): OutputOptions {
  switch (formatId) {
    case "sql_insert":
      return { table_name: "data", dialect: "postgres", batch: true, create_table: false };
    case "template":
      return { template: "" };
    case "csv":
      return { delimiter: "," };
    case "key_value":
      return { separator: ": " };
    case "markdown_list":
      return { bullet: "-" };
    default:
      return {};
  }
}

/** Remove empty strings / undefined so the backend falls back to its defaults. */
export function cleanOptions(options: OutputOptions): OutputOptions {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(options)) {
    if (v === undefined || v === null || v === "") continue;
    out[k] = v;
  }
  return out as OutputOptions;
}

function keyed(lang: Lang, prefix: string, value: unknown, fallback?: string): string {
  const id = String(value ?? "");
  const key = `${prefix}.${id}`;
  const label = translate(lang, key);
  return label === key ? (fallback ?? id.replace(/_/g, " ")) : label;
}

export function stepSummary(step: Step, lang: Lang = "en"): string {
  const s = step as Record<string, unknown>;
  const t = (key: string, params?: Record<string, string | number>) => translate(lang, key, params);
  const list = (v: unknown) => (Array.isArray(v) ? v.join(", ") : String(v ?? ""));
  switch (step.op) {
    case "select_columns":
      return t("step.select_columns", { cols: list(s.columns) });
    case "drop_columns":
      return t("step.drop_columns", { cols: list(s.columns) });
    case "rename_column":
      return t("step.rename_column", { from: String(s.from ?? ""), to: String(s.to ?? "") });
    case "filter_rows":
      return t("step.filter_rows", {
        column: String(s.column ?? ""),
        condition: keyed(lang, "cond", s.condition),
        value: s.value !== undefined && s.value !== null ? ` ${s.value}` : "",
      });
    case "sort_rows":
      return t("step.sort_rows", { column: String(s.column ?? ""), desc: s.descending ? t("step.sort_rows.desc") : "" });
    case "limit":
      return t("step.limit", { count: String(s.count ?? ""), offset: s.offset ? t("step.limit.offset", { offset: String(s.offset) }) : "" });
    case "map_column":
      return t("step.map_column", { transform: keyed(lang, "xf", s.transform), column: String(s.column ?? ""), arg: s.arg ? ` (${s.arg})` : "" });
    case "add_column":
      return t("step.add_column", { name: String(s.name ?? ""), template: String(s.template ?? "") });
    case "split_column":
      return t("step.split_column", { column: String(s.column ?? ""), separator: String(s.separator ?? ""), into: list(s.into) });
    case "dedupe":
      return Array.isArray(s.columns) && s.columns.length ? t("step.dedupe.by", { cols: list(s.columns) }) : t("step.dedupe");
    case "header_case":
      return t("step.header_case", { case: keyed(lang, "case", s.case) });
    case "replace":
      return t("step.replace", {
        pattern: s.regex ? "/" + s.pattern + "/" : `"${s.pattern}"`,
        replacement: String(s.replacement ?? ""),
        column: s.column ? t("step.replace.in", { column: String(s.column) }) : "",
      });
    case "fill_empty":
      return t("step.fill_empty", { column: s.column ? ` ${s.column}` : "", value: String(s.value ?? "") });
    case "text_lines":
      return t("step.text_lines", { transform: keyed(lang, "lines", s.transform) });
    default: {
      const key = `step.${step.op}`;
      const label = translate(lang, key);
      return label === key ? String(step.op).replace(/_/g, " ") : label;
    }
  }
}

export interface ConverterState {
  inputFormat: string; // "auto" or a format id
  parse: ParseOptions;
  steps: Step[];
  outputFormat: string;
  options: OutputOptions;
}

export function buildRecipe(name: string, state: ConverterState, base?: Partial<Recipe>): Recipe {
  return {
    id: base?.id ?? "",
    name: name.trim() || "Untitled recipe",
    description: base?.description ?? "",
    schema_version: 1,
    author: base?.author ?? null,
    tags: base?.tags ?? [],
    created_at: base?.created_at ?? null,
    updated_at: null,
    prompt: base?.prompt ?? null,
    input: {
      format: state.inputFormat === "auto" ? null : state.inputFormat,
      ...cleanParse(state.parse),
    },
    steps: state.steps,
    output: { format: state.outputFormat, ...cleanOptions(state.options) },
    builtin: false,
    ...(base?.translations ? { translations: base.translations } : {}),
  };
}

export function cleanParse(parse: ParseOptions): ParseOptions {
  const out: ParseOptions = {};
  if (parse.delimiter) out.delimiter = parse.delimiter;
  if (parse.has_header !== undefined) out.has_header = parse.has_header;
  if (parse.infer_types !== undefined) out.infer_types = parse.infer_types;
  if (parse.trim !== undefined) out.trim = parse.trim;
  return out;
}

/** The converter state a recipe describes. */
export function stateFromRecipe(recipe: Recipe): ConverterState {
  const { format, ...parse } = recipe.input;
  const { format: outputFormat, ...options } = recipe.output;
  return {
    inputFormat: format ?? "auto",
    parse: cleanParse(parse),
    steps: recipe.steps,
    outputFormat,
    options: cleanOptions(options),
  };
}

export type BadgeTone = "good" | "warn" | "bad" | "neutral";

export function reportBadge(report: ValidationReport | null | undefined, lang: Lang = "en"): { label: string; tone: BadgeTone } {
  const t = (key: string) => translate(lang, key);
  if (!report) return { label: t("report.badge.noResult"), tone: "neutral" };
  if (report.issues.some((i) => i.severity === "error")) return { label: t("report.badge.errors"), tone: "bad" };
  if (report.lossless === true) return { label: t("report.badge.lossless"), tone: "good" };
  if (report.lossless === false) return { label: t("report.badge.changed"), tone: "warn" };
  if (report.issues.some((i) => i.severity === "warning")) return { label: t("report.badge.warnings"), tone: "warn" };
  return { label: t("report.badge.unverifiable"), tone: "neutral" };
}

export function debounce<T extends (...args: never[]) => void>(fn: T, ms: number): T & { cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const wrapped = ((...args: Parameters<T>) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  }) as T & { cancel: () => void };
  wrapped.cancel = () => {
    if (timer) clearTimeout(timer);
  };
  return wrapped;
}

export function shortDate(iso?: string | null, lang: Lang = "en"): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString(lang === "tr" ? "tr-TR" : "en-US", { year: "numeric", month: "short", day: "numeric" });
}

export function delimiterLabel(d?: string, lang: Lang = "en"): string {
  switch (d) {
    case "\t":
      return translate(lang, "delim.tab");
    case ",":
      return translate(lang, "delim.comma");
    case ";":
      return translate(lang, "delim.semicolon");
    case "|":
      return translate(lang, "delim.pipe");
    case undefined:
      return "";
    default:
      return d;
  }
}
