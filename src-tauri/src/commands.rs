//! Tauri commands exposed to the frontend. All arguments use snake_case.
//! Commands that produce user-facing text take an optional `lang` ("en"/"tr").

use crate::state::{AppPaths, AppState, ExportedRecipe};
use pastecompiler_core::ai::AiSettings;
use pastecompiler_core::pipeline::FormatInfo;
use pastecompiler_core::{
    analyze, convert, input_formats, output_formats, Analysis, AppSettings, ConvertRequest,
    ConvertResult, Error, GeneratedRecipe, InputFormat, Lang, ParseOptions, Recipe,
};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct Formats {
    pub inputs: Vec<FormatInfo>,
    pub outputs: Vec<FormatInfo>,
}

fn enter(lang: &Option<String>) {
    Lang::enter(lang.as_deref());
}

#[tauri::command(rename_all = "snake_case")]
pub fn analyze_input(
    input: String,
    input_format: Option<String>,
    parse: Option<ParseOptions>,
    lang: Option<String>,
) -> Result<Analysis, Error> {
    enter(&lang);
    let forced = match input_format
        .as_deref()
        .filter(|f| !f.is_empty() && *f != "auto")
    {
        Some(f) => Some(InputFormat::parse_id(f).ok_or_else(|| {
            Error::Unsupported(pastecompiler_core::i18n::t(
                "err_unknown_input_format",
                &[&f],
            ))
        })?),
        None => None,
    };
    analyze(&input, forced, &parse.unwrap_or_default())
}

#[tauri::command(rename_all = "snake_case")]
pub fn convert_input(request: ConvertRequest) -> Result<ConvertResult, Error> {
    Lang::set(request.lang.unwrap_or_default());
    convert(&request)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_formats() -> Formats {
    Formats {
        inputs: input_formats(),
        outputs: output_formats(),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_recipes(state: State<'_, AppState>) -> Result<Vec<Recipe>, Error> {
    state.store.list_all()
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_recipe(
    state: State<'_, AppState>,
    id: String,
    lang: Option<String>,
) -> Result<Recipe, Error> {
    enter(&lang);
    state.store.get(&id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_recipe(
    state: State<'_, AppState>,
    recipe: Recipe,
    lang: Option<String>,
) -> Result<Recipe, Error> {
    enter(&lang);
    state.store.save(recipe)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_recipe(
    state: State<'_, AppState>,
    id: String,
    lang: Option<String>,
) -> Result<(), Error> {
    enter(&lang);
    state.store.delete(&id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn run_recipe(
    state: State<'_, AppState>,
    input: String,
    id: Option<String>,
    recipe: Option<Recipe>,
    lang: Option<String>,
) -> Result<ConvertResult, Error> {
    enter(&lang);
    state.run_recipe(id.as_deref(), recipe, &input)
}

#[tauri::command(rename_all = "snake_case")]
pub fn export_recipe(
    state: State<'_, AppState>,
    id: String,
    lang: Option<String>,
) -> Result<ExportedRecipe, Error> {
    enter(&lang);
    state.export_recipe(&id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn import_recipe(
    state: State<'_, AppState>,
    text: String,
    lang: Option<String>,
) -> Result<Recipe, Error> {
    enter(&lang);
    state.store.import(&text)
}

#[tauri::command(rename_all = "snake_case")]
pub fn parse_recipe(text: String, lang: Option<String>) -> Result<Recipe, Error> {
    enter(&lang);
    Recipe::from_share_string(&text)
}

#[tauri::command(rename_all = "snake_case")]
pub fn export_recipe_to_file(
    state: State<'_, AppState>,
    id: String,
    path: String,
    lang: Option<String>,
) -> Result<(), Error> {
    enter(&lang);
    state.export_recipe_to_file(&id, &PathBuf::from(path))
}

#[tauri::command(rename_all = "snake_case")]
pub fn import_recipe_from_file(
    state: State<'_, AppState>,
    path: String,
    lang: Option<String>,
) -> Result<Recipe, Error> {
    enter(&lang);
    state.import_recipe_from_file(&PathBuf::from(path))
}

#[tauri::command(rename_all = "snake_case")]
pub fn write_text_file(path: String, contents: String) -> Result<(), Error> {
    std::fs::write(path, contents)?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings()
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, Error> {
    state.save_settings(settings)
}

#[tauri::command(rename_all = "snake_case")]
pub fn app_paths(state: State<'_, AppState>) -> AppPaths {
    state.paths()
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ai_generate_recipe(
    state: State<'_, AppState>,
    instruction: String,
    sample: String,
    existing: Option<Recipe>,
    lang: Option<String>,
) -> Result<GeneratedRecipe, Error> {
    enter(&lang);
    state.generate_recipe(&instruction, &sample, existing).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ai_test_connection(
    state: State<'_, AppState>,
    settings: Option<AiSettings>,
    lang: Option<String>,
) -> Result<String, Error> {
    enter(&lang);
    let settings = settings.unwrap_or_else(|| state.ai_settings());
    pastecompiler_core::ai::test_connection(&settings).await
}
