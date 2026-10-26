import { describe, expect, it } from "vitest";
import {
  buildRecipe,
  cleanOptions,
  confidenceLabel,
  defaultOptionsFor,
  delimiterLabel,
  fileNameFor,
  formatBytes,
  formatLabel,
  optionFieldsFor,
  providerLabel,
  recipeDescription,
  recipeName,
  reportBadge,
  stateFromRecipe,
  stepSummary,
} from "./utils";
import type { Recipe, ValidationReport } from "./types";

describe("labels", () => {
  it("maps confidence to labels", () => {
    expect(confidenceLabel(0.95)).toBe("high");
    expect(confidenceLabel(0.7)).toBe("medium");
    expect(confidenceLabel(0.4)).toBe("low");
  });

  it("formats bytes and file names", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(3 * 1024 * 1024)).toBe("3.0 MB");
    expect(fileNameFor("sql_insert", "sql")).toBe("pastecompiler-sql-insert.sql");
    expect(fileNameFor("text", "")).toBe("pastecompiler-text.txt");
    expect(delimiterLabel("\t")).toBe("tab");
    expect(delimiterLabel(";")).toBe("semicolon");
    expect(delimiterLabel(";", "tr")).toBe("noktalı virgül");
  });

  it("summarises steps in both languages", () => {
    expect(stepSummary({ op: "select_columns", columns: ["a", "b"] })).toBe("keep columns a, b");
    expect(stepSummary({ op: "filter_rows", column: "age", condition: "gt", value: "30" })).toBe("filter age > 30");
    expect(stepSummary({ op: "sort_rows", column: "name", descending: true })).toBe("sort by name (desc)");
    expect(stepSummary({ op: "dedupe", columns: [] })).toBe("remove duplicate rows");
    expect(stepSummary({ op: "transpose" })).toBe("transpose");
    expect(stepSummary({ op: "map_column", column: "x", transform: "snake_case" })).toBe("snake_case x");
    expect(stepSummary({ op: "sort_rows", column: "name", descending: true }, "tr")).toBe("name alanına göre sırala (azalan)");
    expect(stepSummary({ op: "text_lines", transform: "remove_empty" }, "tr")).toBe("satırlar: boş satırları kaldır");
    expect(stepSummary({ op: "some_new_op" }, "tr")).toBe("some new op");
  });

  it("localizes format, category and provider labels", () => {
    expect(formatLabel("markdown_table", "en")).toBe("Markdown table");
    expect(formatLabel("markdown_table", "tr")).toBe("Markdown tablosu");
    expect(formatLabel("unknown_format", "tr", "Backend label")).toBe("Backend label");
    expect(formatLabel("unknown_format", "en")).toBe("unknown format");
    expect(providerLabel("ollama", "tr")).toBe("Ollama (yerel)");
    expect(providerLabel("mystery", "en")).toBe("mystery");
  });
});

describe("options", () => {
  it("has fields and defaults per format", () => {
    expect(optionFieldsFor("sql_insert").map((f) => f.key)).toEqual(["table_name", "dialect", "create_table", "batch"]);
    expect(optionFieldsFor("yaml")).toEqual([]);
    expect(optionFieldsFor("csv").find((f) => f.key === "header")?.defaultValue).toBe(true);
    expect(optionFieldsFor("sql_insert", "tr")[0].label).toBe("Tablo adı");
    expect(optionFieldsFor("template", "en")[0].help).toContain("{{column}}");
    expect(defaultOptionsFor("sql_insert").table_name).toBe("data");
    expect(defaultOptionsFor("json")).toEqual({});
  });

  it("cleans empty values", () => {
    expect(cleanOptions({ indent: 4, table_name: "", template: undefined, batch: false })).toEqual({ indent: 4, batch: false });
  });
});

describe("recipes", () => {
  const recipe: Recipe = {
    id: "r1",
    name: "Test",
    description: "d",
    schema_version: 1,
    tags: ["x"],
    input: { format: "csv", delimiter: ";" },
    steps: [{ op: "limit", count: 3, offset: 0 }],
    output: { format: "sql_insert", table_name: "people", batch: false },
    builtin: false,
  };

  it("converts between recipe and converter state", () => {
    const state = stateFromRecipe(recipe);
    expect(state.inputFormat).toBe("csv");
    expect(state.parse).toEqual({ delimiter: ";" });
    expect(state.outputFormat).toBe("sql_insert");
    expect(state.options).toEqual({ table_name: "people", batch: false });
    const back = buildRecipe("Renamed", state, recipe);
    expect(back.id).toBe("r1");
    expect(back.name).toBe("Renamed");
    expect(back.input).toEqual({ format: "csv", delimiter: ";" });
    expect(back.output).toEqual({ format: "sql_insert", table_name: "people", batch: false });
    expect(back.steps).toEqual(recipe.steps);
  });

  it("uses auto input when no format is forced", () => {
    const r = buildRecipe("  ", { inputFormat: "auto", parse: {}, steps: [], outputFormat: "json", options: { indent: 2, table_name: "" } });
    expect(r.name).toBe("Untitled recipe");
    expect(r.input.format).toBeNull();
    expect(r.output).toEqual({ format: "json", indent: 2 });
  });

  it("falls back to the base name when no translation exists", () => {
    expect(recipeName(recipe, "tr")).toBe("Test");
    const translated: Recipe = { ...recipe, translations: { tr: { name: "Deneme", description: "açıklama" } } };
    expect(recipeName(translated, "tr")).toBe("Deneme");
    expect(recipeName(translated, "en")).toBe("Test");
    expect(recipeDescription(translated, "tr")).toBe("açıklama");
    expect(recipeDescription(translated, "en")).toBe("d");
    expect(buildRecipe("x", stateFromRecipe(translated), translated).translations).toEqual(translated.translations);
  });
});

describe("report badge", () => {
  const base: ValidationReport = { lossless: true, round_trip: "verified", issues: [], differences: [], stats: { input_bytes: 1, output_bytes: 1, elapsed_ms: 0 } };
  it("picks a tone", () => {
    expect(reportBadge(null).tone).toBe("neutral");
    expect(reportBadge(base)).toEqual({ label: "Lossless", tone: "good" });
    expect(reportBadge(base, "tr")).toEqual({ label: "Kayıpsız", tone: "good" });
    expect(reportBadge({ ...base, lossless: false }).tone).toBe("warn");
    expect(reportBadge({ ...base, lossless: null, round_trip: "unavailable" }).label).toBe("Not verifiable");
    expect(reportBadge({ ...base, issues: [{ severity: "error", code: "x", message: "m" }] }).tone).toBe("bad");
  });
});
