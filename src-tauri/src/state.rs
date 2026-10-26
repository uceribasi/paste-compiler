//! Application state shared by all Tauri commands.

use pastecompiler_core::ai::AiSettings;
use pastecompiler_core::{AppSettings, Error, GeneratedRecipe, Recipe, RecipeStore, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct AppPaths {
    pub data_dir: String,
    pub recipes_dir: String,
    pub settings_file: String,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub store: RecipeStore,
    pub settings_path: PathBuf,
    pub settings: Mutex<AppSettings>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportedRecipe {
    pub recipe: Recipe,
    pub json: String,
    pub share: String,
}

impl AppState {
    pub fn new(data_dir: impl Into<PathBuf>) -> AppState {
        let data_dir: PathBuf = data_dir.into();
        let settings_path = data_dir.join("settings.json");
        let settings = AppSettings::load(&settings_path).unwrap_or_default();
        AppState {
            store: RecipeStore::new(data_dir.join("recipes")),
            settings_path,
            settings: Mutex::new(settings),
            data_dir,
        }
    }

    pub fn paths(&self) -> AppPaths {
        AppPaths {
            data_dir: self.data_dir.display().to_string(),
            recipes_dir: self.store.dir().display().to_string(),
            settings_file: self.settings_path.display().to_string(),
        }
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn ai_settings(&self) -> AiSettings {
        self.settings().ai
    }

    pub fn save_settings(&self, settings: AppSettings) -> Result<AppSettings> {
        settings.save(&self.settings_path)?;
        if let Ok(mut guard) = self.settings.lock() {
            *guard = settings.clone();
        }
        Ok(settings)
    }

    pub fn export_recipe(&self, id: &str) -> Result<ExportedRecipe> {
        let recipe = self.store.get(id)?;
        Ok(ExportedRecipe {
            json: recipe.to_json(),
            share: recipe.to_share_string(),
            recipe,
        })
    }

    pub fn run_recipe(
        &self,
        id: Option<&str>,
        recipe: Option<Recipe>,
        input: &str,
    ) -> Result<pastecompiler_core::ConvertResult> {
        let recipe = match (recipe, id) {
            (Some(r), _) => r,
            (None, Some(id)) => self.store.get(id)?,
            (None, None) => return Err(Error::Recipe("no recipe given".into())),
        };
        recipe.run(input)
    }

    pub fn export_recipe_to_file(&self, id: &str, path: &Path) -> Result<()> {
        self.store.export_file(id, path)
    }

    pub fn import_recipe_from_file(&self, path: &Path) -> Result<Recipe> {
        self.store.import_file(path)
    }

    pub async fn generate_recipe(
        &self,
        instruction: &str,
        sample: &str,
        existing: Option<Recipe>,
    ) -> Result<GeneratedRecipe> {
        let settings = self.ai_settings();
        pastecompiler_core::generate_recipe(&settings, instruction, sample, existing.as_ref()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pastecompiler_core::OutputFormat;

    #[test]
    fn state_persists_settings_and_recipes() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(dir.path());
        assert_eq!(state.settings(), AppSettings::default());
        let mut s = state.settings();
        s.theme = "dark".into();
        state.save_settings(s.clone()).unwrap();
        assert_eq!(AppState::new(dir.path()).settings().theme, "dark");

        let saved = state
            .store
            .save(Recipe::new("Mine", OutputFormat::Csv))
            .unwrap();
        let exported = state.export_recipe(&saved.id).unwrap();
        assert!(exported.share.starts_with("pcr1."));
        let out = state.run_recipe(Some(&saved.id), None, "a,b\n1,2").unwrap();
        assert_eq!(out.output, "a,b\n1,2");
        let file = dir.path().join("x.json");
        state.export_recipe_to_file(&saved.id, &file).unwrap();
        let imported = state.import_recipe_from_file(&file).unwrap();
        assert_ne!(imported.id, saved.id);
        assert!(state.run_recipe(None, None, "x").is_err());
        let paths = state.paths();
        assert!(paths.recipes_dir.ends_with("recipes"));
    }

    #[tokio::test]
    async fn offline_generation_works_through_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(dir.path());
        let g = state
            .generate_recipe("convert to yaml", "a,b\n1,2", None)
            .await
            .unwrap();
        assert_eq!(g.recipe.output.format, OutputFormat::Yaml);
    }
}
