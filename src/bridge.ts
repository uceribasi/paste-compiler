// Thin wrapper around Tauri's invoke + plugins, with a browser fallback so the
// UI can be opened in a plain browser during development.

import { getCurrentLang, translate, type Lang } from "./i18n";
import type {
  AiSettings,
  Analysis,
  AppPaths,
  AppSettings,
  ConvertRequest,
  ConvertResult,
  ExportedRecipe,
  Formats,
  GeneratedRecipe,
  ParseOptions,
  Recipe,
} from "./types";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const DEV_SERVER = (import.meta.env.VITE_DEV_SERVER as string | undefined) ?? "http://127.0.0.1:4567";

/** Explains how to get the engine running when the UI is opened outside Tauri. */
export function engineHint(): string {
  return translate(getCurrentLang(), "engine.hint");
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    try {
      return await invoke<T>(cmd, args);
    } catch (e) {
      throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));
    }
  }
  // Browser dev mode: talk to the HTTP dev server (see crates/pastecompiler-core/examples/devserver.rs).
  let response: Response;
  try {
    response = await fetch(`${DEV_SERVER}/invoke/${cmd}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args ?? {}),
    });
  } catch {
    throw new Error(engineHint());
  }
  const text = await response.text();
  const data = text ? (JSON.parse(text) as unknown) : null;
  if (!response.ok) {
    const err = (data as { error?: unknown } | null)?.error;
    throw new Error(typeof err === "string" ? err : text);
  }
  return data as T;
}

export const api = {
  analyze: (input: string, inputFormat: string, parse: ParseOptions, lang: Lang) =>
    call<Analysis>("analyze_input", { input, input_format: inputFormat === "auto" ? null : inputFormat, parse, lang }),
  convert: (request: ConvertRequest) => call<ConvertResult>("convert_input", { request }),
  listFormats: () => call<Formats>("list_formats"),
  listRecipes: () => call<Recipe[]>("list_recipes"),
  getRecipe: (id: string) => call<Recipe>("get_recipe", { id }),
  saveRecipe: (recipe: Recipe, lang: Lang) => call<Recipe>("save_recipe", { recipe, lang }),
  deleteRecipe: (id: string) => call<void>("delete_recipe", { id }),
  runRecipe: (input: string, id: string | undefined, recipe: Recipe | undefined, lang: Lang) =>
    call<ConvertResult>("run_recipe", { input, id: id ?? null, recipe: recipe ?? null, lang }),
  exportRecipe: (id: string) => call<ExportedRecipe>("export_recipe", { id }),
  importRecipe: (text: string, lang: Lang) => call<Recipe>("import_recipe", { text, lang }),
  parseRecipe: (text: string, lang: Lang) => call<Recipe>("parse_recipe", { text, lang }),
  exportRecipeToFile: (id: string, path: string) => call<void>("export_recipe_to_file", { id, path }),
  importRecipeFromFile: (path: string, lang: Lang) => call<Recipe>("import_recipe_from_file", { path, lang }),
  writeTextFile: (path: string, contents: string) => call<void>("write_text_file", { path, contents }),
  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<AppSettings>("save_settings", { settings }),
  appPaths: () => call<AppPaths>("app_paths"),
  aiGenerate: (instruction: string, sample: string, existing: Recipe | undefined, lang: Lang) =>
    call<GeneratedRecipe>("ai_generate_recipe", { instruction, sample, existing: existing ?? null, lang }),
  aiTestConnection: (settings: AiSettings | undefined, lang: Lang) => call<string>("ai_test_connection", { settings: settings ?? null, lang }),
};

export async function readClipboard(): Promise<string> {
  if (isTauri()) {
    const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
    return (await readText()) ?? "";
  }
  if (navigator.clipboard?.readText) {
    return navigator.clipboard.readText();
  }
  throw new Error(translate(getCurrentLang(), "bridge.noClipboard"));
}

export async function writeClipboard(text: string): Promise<void> {
  if (isTauri()) {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    await writeText(text);
    return;
  }
  await navigator.clipboard.writeText(text);
}

export async function saveTextToFile(suggestedName: string, contents: string, extension: string): Promise<string | null> {
  if (!isTauri()) {
    const blob = new Blob([contents], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = suggestedName;
    a.click();
    URL.revokeObjectURL(url);
    return suggestedName;
  }
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ defaultPath: suggestedName, filters: extension ? [{ name: extension.toUpperCase(), extensions: [extension] }] : undefined });
  if (!path) return null;
  await api.writeTextFile(path, contents);
  return path;
}

export async function pickRecipeFile(): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ multiple: false, filters: [{ name: "Recipe", extensions: ["json"] }] });
  if (!picked) return null;
  return typeof picked === "string" ? picked : null;
}

export async function pickSavePath(suggestedName: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ defaultPath: suggestedName, filters: [{ name: "Recipe", extensions: ["json"] }] });
  return path ?? null;
}

export async function confirmDialog(message: string, title = "PasteCompiler"): Promise<boolean> {
  if (!isTauri()) return window.confirm(message);
  const { ask } = await import("@tauri-apps/plugin-dialog");
  return ask(message, { title, kind: "warning" });
}
