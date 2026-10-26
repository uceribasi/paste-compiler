//! Development server that exposes the core over plain HTTP so the React UI can
//! be used in a normal browser without Tauri:
//!
//! ```bash
//! cargo run -p pastecompiler-core --example devserver   # http://127.0.0.1:4567
//! npm run dev                                           # http://localhost:1420
//! ```
//!
//! Every Tauri command is available as `POST /invoke/<command>` with the
//! arguments as a JSON object. Recipes and settings live in a separate
//! directory (`PASTECOMPILER_DATA_DIR`, default: a `pastecompiler-dev` folder
//! in the temp dir) so they never mix with the desktop app's data.

use pastecompiler_core::ai::{test_connection, AiSettings};
use pastecompiler_core::{
    analyze, convert, generate_recipe, input_formats, output_formats, AppSettings, ConvertRequest,
    Error, InputFormat, ParseOptions, Recipe, RecipeStore, Result,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tiny_http::{Header, Method, Request, Response, Server};

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn respond(req: Request, status: u16, body: &str) {
    let mut response = Response::from_string(body).with_status_code(status);
    for h in [
        header("Access-Control-Allow-Origin", "*"),
        header("Access-Control-Allow-Headers", "content-type"),
        header("Access-Control-Allow-Methods", "POST, OPTIONS"),
        header("Content-Type", "application/json"),
    ] {
        response.add_header(h);
    }
    let _ = req.respond(response);
}

fn handle(
    cmd: &str,
    args: Value,
    store: &RecipeStore,
    settings_path: &Path,
    rt: &tokio::runtime::Runtime,
) -> Result<Value> {
    let get = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
    let s = |k: &str| get(k).as_str().map(str::to_string).unwrap_or_default();
    Ok(match cmd {
        "analyze_input" => {
            let forced = match get("input_format")
                .as_str()
                .filter(|f| !f.is_empty() && *f != "auto")
            {
                Some(f) => Some(
                    InputFormat::parse_id(f)
                        .ok_or_else(|| Error::Unsupported(format!("unknown input format '{f}'")))?,
                ),
                None => None,
            };
            let parse: ParseOptions = serde_json::from_value(get("parse")).unwrap_or_default();
            serde_json::to_value(analyze(&s("input"), forced, &parse)?)?
        }
        "convert_input" => {
            let request: ConvertRequest = serde_json::from_value(get("request"))?;
            serde_json::to_value(convert(&request)?)?
        }
        "list_formats" => json!({ "inputs": input_formats(), "outputs": output_formats() }),
        "list_recipes" => serde_json::to_value(store.list_all()?)?,
        "get_recipe" => serde_json::to_value(store.get(&s("id"))?)?,
        "save_recipe" => {
            let recipe: Recipe = serde_json::from_value(get("recipe"))?;
            serde_json::to_value(store.save(recipe)?)?
        }
        "delete_recipe" => {
            store.delete(&s("id"))?;
            Value::Null
        }
        "run_recipe" => {
            let recipe: Option<Recipe> = serde_json::from_value(get("recipe")).ok();
            let recipe = match (recipe, get("id").as_str()) {
                (Some(r), _) => r,
                (None, Some(id)) => store.get(id)?,
                (None, None) => return Err(Error::Recipe("no recipe given".into())),
            };
            serde_json::to_value(recipe.run(&s("input"))?)?
        }
        "export_recipe" => {
            let recipe = store.get(&s("id"))?;
            json!({ "json": recipe.to_json(), "share": recipe.to_share_string(), "recipe": recipe })
        }
        "import_recipe" => serde_json::to_value(store.import(&s("text"))?)?,
        "parse_recipe" => serde_json::to_value(Recipe::from_share_string(&s("text"))?)?,
        "export_recipe_to_file" => {
            store.export_file(&s("id"), Path::new(&s("path")))?;
            Value::Null
        }
        "import_recipe_from_file" => {
            serde_json::to_value(store.import_file(Path::new(&s("path")))?)?
        }
        "write_text_file" => {
            std::fs::write(s("path"), s("contents"))?;
            Value::Null
        }
        "get_settings" => serde_json::to_value(AppSettings::load(settings_path)?)?,
        "save_settings" => {
            let settings: AppSettings = serde_json::from_value(get("settings"))?;
            settings.save(settings_path)?;
            serde_json::to_value(settings)?
        }
        "app_paths" => json!({
            "data_dir": settings_path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
            "recipes_dir": store.dir().display().to_string(),
            "settings_file": settings_path.display().to_string(),
        }),
        "ai_generate_recipe" => {
            let settings = AppSettings::load(settings_path)?.ai;
            let existing: Option<Recipe> = serde_json::from_value(get("existing")).ok();
            let generated = rt.block_on(generate_recipe(
                &settings,
                &s("instruction"),
                &s("sample"),
                existing.as_ref(),
            ))?;
            serde_json::to_value(generated)?
        }
        "ai_test_connection" => {
            let settings: AiSettings = match serde_json::from_value(get("settings")) {
                Ok(s) => s,
                Err(_) => AppSettings::load(settings_path)?.ai,
            };
            Value::String(rt.block_on(test_connection(&settings))?)
        }
        other => return Err(Error::Unsupported(format!("unknown command '{other}'"))),
    })
}

fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(4567);
    let data_dir = std::env::var("PASTECOMPILER_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("pastecompiler-dev"));
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    let store = RecipeStore::new(data_dir.join("recipes"));
    let settings_path = data_dir.join("settings.json");
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let server = Server::http(("127.0.0.1", port)).expect("bind dev server");
    println!(
        "PasteCompiler dev server listening on http://127.0.0.1:{port} (data: {})",
        data_dir.display()
    );

    for mut req in server.incoming_requests() {
        if *req.method() == Method::Options {
            respond(req, 204, "");
            continue;
        }
        let cmd = req
            .url()
            .trim_start_matches('/')
            .trim_start_matches("invoke/")
            .to_string();
        let mut body = String::new();
        let _ = req.as_reader().read_to_string(&mut body);
        let args: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
        pastecompiler_core::Lang::enter(args.get("lang").and_then(|l| l.as_str()));
        match handle(&cmd, args, &store, &settings_path, &rt) {
            Ok(v) => respond(req, 200, &v.to_string()),
            Err(e) => respond(req, 400, &json!({ "error": e.to_string() }).to_string()),
        }
    }
}
