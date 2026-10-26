//! File-based storage for recipes and built-in example recipes.

use crate::error::{Error, Result};
use crate::recipe::{parse_recipe_lenient, Recipe};
use std::fs;
use std::path::{Path, PathBuf};

/// Built-in example recipes shipped with the app (see the `recipes/` folder).
pub const BUILTIN_RECIPES: &[&str] = &[
    include_str!("../../../recipes/csv-to-sql-inserts.json"),
    include_str!("../../../recipes/json-to-markdown-table.json"),
    include_str!("../../../recipes/table-to-excel.json"),
    include_str!("../../../recipes/snake-case-json.json"),
    include_str!("../../../recipes/lines-to-json-array.json"),
    include_str!("../../../recipes/env-to-yaml.json"),
    include_str!("../../../recipes/emails-template.json"),
    include_str!("../../../recipes/dedupe-sorted-lines.json"),
];

pub fn builtin_recipes() -> Vec<Recipe> {
    BUILTIN_RECIPES
        .iter()
        .filter_map(|text| parse_recipe_lenient(text).ok())
        .map(|mut r| {
            r.builtin = true;
            r
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct RecipeStore {
    dir: PathBuf,
}

fn safe_file_name(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        cleaned
    }
}

impl RecipeStore {
    pub fn new(dir: impl Into<PathBuf>) -> RecipeStore {
        RecipeStore { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn ensure_dir(&self) -> Result<()> {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir)?;
        }
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", safe_file_name(id)))
    }

    /// User recipes sorted by name.
    pub fn list(&self) -> Result<Vec<Recipe>> {
        let mut out = Vec::new();
        if !self.dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = fs::read_to_string(&path)?;
            match parse_recipe_lenient(&text) {
                Ok(mut r) => {
                    r.builtin = false;
                    out.push(r)
                }
                Err(e) => eprintln!("skipping invalid recipe {}: {e}", path.display()),
            }
        }
        out.sort_by_key(|r| r.name.to_lowercase());
        Ok(out)
    }

    /// User recipes followed by the built-in ones.
    pub fn list_all(&self) -> Result<Vec<Recipe>> {
        let mut all = self.list()?;
        let user_ids: Vec<String> = all.iter().map(|r| r.id.clone()).collect();
        all.extend(
            builtin_recipes()
                .into_iter()
                .filter(|b| !user_ids.contains(&b.id)),
        );
        Ok(all)
    }

    pub fn get(&self, id: &str) -> Result<Recipe> {
        let path = self.path_for(id);
        if path.exists() {
            let text = fs::read_to_string(&path)?;
            return parse_recipe_lenient(&text).map(|mut r| {
                r.builtin = false;
                r
            });
        }
        builtin_recipes()
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| Error::NotFound(crate::i18n::t("err_recipe_not_found", &[&id])))
    }

    pub fn save(&self, mut recipe: Recipe) -> Result<Recipe> {
        let problems = recipe.check();
        if !problems.is_empty() {
            return Err(Error::Recipe(problems.join("; ")));
        }
        self.ensure_dir()?;
        if recipe.id.trim().is_empty() {
            recipe.id = uuid::Uuid::new_v4().to_string();
        }
        recipe.builtin = false;
        recipe.touch();
        fs::write(self.path_for(&recipe.id), recipe.to_json())?;
        Ok(recipe)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(Error::NotFound(crate::i18n::t(
                "err_recipe_not_found",
                &[&id],
            )));
        }
        fs::remove_file(path)?;
        Ok(())
    }

    /// Import from a share string or JSON. A fresh id is assigned when the id
    /// collides with an existing recipe, so imports never overwrite silently.
    pub fn import(&self, text: &str) -> Result<Recipe> {
        let mut recipe = Recipe::from_share_string(text)?;
        if self.path_for(&recipe.id).exists() || builtin_recipes().iter().any(|b| b.id == recipe.id)
        {
            recipe.id = uuid::Uuid::new_v4().to_string();
            let suffix = crate::i18n::t("imported_suffix", &[]);
            if !recipe.name.ends_with(&suffix) {
                recipe.name = format!("{} {suffix}", recipe.name);
            }
        }
        self.save(recipe)
    }

    pub fn import_file(&self, path: &Path) -> Result<Recipe> {
        let text = fs::read_to_string(path)?;
        self.import(&text)
    }

    pub fn export_file(&self, id: &str, path: &Path) -> Result<()> {
        let recipe = self.get(id)?;
        fs::write(path, recipe.to_json())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OutputFormat;

    fn temp_store() -> RecipeStore {
        let dir = std::env::temp_dir().join(format!("pastecompiler-test-{}", uuid::Uuid::new_v4()));
        RecipeStore::new(dir)
    }

    #[test]
    fn builtin_recipes_parse_and_check() {
        let recipes = builtin_recipes();
        assert_eq!(
            recipes.len(),
            BUILTIN_RECIPES.len(),
            "every built-in recipe must parse"
        );
        for r in &recipes {
            assert!(r.builtin);
            assert!(r.check().is_empty(), "{}: {:?}", r.name, r.check());
            assert!(!r.id.is_empty());
        }
        let mut ids: Vec<&str> = recipes.iter().map(|r| r.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            recipes.len(),
            "built-in recipe ids must be unique"
        );
    }

    #[test]
    fn save_list_get_delete() {
        let store = temp_store();
        assert!(store.list().unwrap().is_empty());
        let r = store
            .save(Recipe::new("B recipe", OutputFormat::Csv))
            .unwrap();
        store
            .save(Recipe::new("A recipe", OutputFormat::Json))
            .unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "A recipe");
        assert_eq!(store.get(&r.id).unwrap().name, "B recipe");
        assert!(store.list_all().unwrap().len() > 2);
        store.delete(&r.id).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        assert!(store.get(&r.id).is_err());
        assert!(store.delete("missing").is_err());
        let _ = fs::remove_dir_all(store.dir());
    }

    #[test]
    fn import_assigns_new_id_on_collision() {
        let store = temp_store();
        let saved = store
            .save(Recipe::new("Shared", OutputFormat::Yaml))
            .unwrap();
        let imported = store.import(&saved.to_share_string()).unwrap();
        assert_ne!(imported.id, saved.id);
        assert_eq!(imported.name, "Shared (imported)");
        assert_eq!(store.list().unwrap().len(), 2);
        let builtin = builtin_recipes().remove(0);
        let imported_builtin = store.import(&builtin.to_json()).unwrap();
        assert_ne!(imported_builtin.id, builtin.id);
        let _ = fs::remove_dir_all(store.dir());
    }

    #[test]
    fn export_and_import_files() {
        let store = temp_store();
        let saved = store.save(Recipe::new("File", OutputFormat::Toml)).unwrap();
        let path = store.dir().join("export.recipe.json");
        store.export_file(&saved.id, &path).unwrap();
        let imported = store.import_file(&path).unwrap();
        assert_eq!(imported.output.format, OutputFormat::Toml);
        let _ = fs::remove_dir_all(store.dir());
    }

    #[test]
    fn empty_id_gets_assigned_once() {
        let store = temp_store();
        let mut r = Recipe::new("No id", OutputFormat::Json);
        r.id = String::new();
        let saved = store.save(r).unwrap();
        assert!(!saved.id.is_empty());
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].id, saved.id,
            "the stored id must be stable across reads"
        );
        assert_eq!(store.get(&saved.id).unwrap().name, "No id");
        let _ = fs::remove_dir_all(store.dir());
    }

    #[test]
    fn invalid_recipe_is_rejected() {
        let store = temp_store();
        let bad = Recipe::new("", OutputFormat::Json);
        assert!(store.save(bad).is_err());
    }
}
