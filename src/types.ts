// Types mirroring the Rust structs serialized by pastecompiler-core.

import type { Lang } from "./i18n";

export type Severity = "info" | "warning" | "error";

export interface Issue {
  severity: Severity;
  code: string;
  message: string;
  location?: string;
}

export interface Candidate {
  format: string;
  confidence: number;
  reason: string;
}

export interface Detection {
  format: string;
  confidence: number;
  delimiter?: string;
  has_header?: boolean;
  reasons: string[];
  candidates: Candidate[];
}

export type DocKind = "table" | "tree" | "list" | "text";

export interface DocSummary {
  kind: DocKind;
  rows?: number | null;
  columns?: number | null;
  headers?: string[] | null;
  has_header?: boolean | null;
  items?: number | null;
  tree_kind?: string | null;
  column_types?: string[] | null;
}

export interface FormatInfo {
  id: string;
  label: string;
  category: string;
  extension: string;
}

export interface TablePreview {
  headers: string[];
  rows: string[][];
  total_rows: number;
}

export interface Analysis {
  detection: Detection;
  summary: DocSummary;
  issues: Issue[];
  suggestions: FormatInfo[];
  preview?: TablePreview;
}

export interface ParseOptions {
  delimiter?: string;
  has_header?: boolean;
  infer_types?: boolean;
  trim?: boolean;
}

export interface OutputOptions {
  indent?: number;
  delimiter?: string;
  quote_all?: boolean;
  header?: boolean;
  crlf?: boolean;
  null_as?: string;
  table_name?: string;
  dialect?: string;
  create_table?: boolean;
  batch?: boolean;
  root_name?: string;
  item_name?: string;
  declaration?: boolean;
  template?: string;
  separator?: string;
  prefix?: string;
  suffix?: string;
  bullet?: string;
  align?: boolean;
}

export interface Step {
  op: string;
  [key: string]: unknown;
}

export interface ConvertRequest {
  input: string;
  input_format?: string;
  parse: ParseOptions;
  steps: Step[];
  output?: string;
  options: OutputOptions;
  /** UI language; engine messages are localized to it. */
  lang?: Lang;
}

export type RoundTrip = "verified" | "differences" | "failed" | "unavailable";

export interface Stats {
  input_bytes: number;
  output_bytes: number;
  input_rows?: number | null;
  output_rows?: number | null;
  input_columns?: number | null;
  output_columns?: number | null;
  elapsed_ms: number;
}

export interface ValidationReport {
  lossless: boolean | null;
  round_trip: RoundTrip;
  issues: Issue[];
  differences: string[];
  stats: Stats;
}

export interface ConvertResult {
  output: string;
  output_format: string;
  detection: Detection;
  input_summary: DocSummary;
  result_summary: DocSummary;
  report: ValidationReport;
}

export interface RecipeInput extends ParseOptions {
  format?: string | null;
}

export interface RecipeOutput extends OutputOptions {
  format: string;
}

export interface Recipe {
  id: string;
  name: string;
  description: string;
  schema_version: number;
  author?: string | null;
  tags: string[];
  created_at?: string | null;
  updated_at?: string | null;
  prompt?: string | null;
  input: RecipeInput;
  steps: Step[];
  output: RecipeOutput;
  builtin: boolean;
  /** Localized name/description keyed by language code ("tr", ...). */
  translations?: Record<string, { name: string; description: string }>;
}

export interface ExportedRecipe {
  recipe: Recipe;
  json: string;
  share: string;
}

export type Provider = "rules" | "ollama" | "anthropic" | "openai_compatible";

export interface AiSettings {
  provider: Provider;
  base_url: string;
  model: string;
  api_key: string;
  include_sample: boolean;
  sample_lines: number;
  timeout_secs: number;
  fallback_to_rules: boolean;
}

export interface AppSettings {
  ai: AiSettings;
  theme: "system" | "light" | "dark" | string;
  /** "system" | "en" | "tr" */
  language: string;
  auto_paste_on_focus: boolean;
  watch_clipboard: boolean;
  auto_copy_result: boolean;
  default_output?: string | null;
  settings_version: number;
}

export interface GeneratedRecipe {
  recipe: Recipe;
  explanation: string;
  provider: Provider;
  model: string;
  attempts: number;
  used_fallback: boolean;
  fallback_reason?: string;
  test_output?: string;
  test_report?: ValidationReport;
}

export interface Formats {
  inputs: FormatInfo[];
  outputs: FormatInfo[];
}

export interface AppPaths {
  data_dir: string;
  recipes_dir: string;
  settings_file: string;
}
