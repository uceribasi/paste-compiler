//! PasteCompiler desktop application (Tauri shell around `pastecompiler-core`).

mod commands;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("pastecompiler"));
            if let Err(e) = std::fs::create_dir_all(&data_dir) {
                eprintln!("could not create data dir {}: {e}", data_dir.display());
            }
            app.manage(AppState::new(data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::analyze_input,
            commands::convert_input,
            commands::list_formats,
            commands::list_recipes,
            commands::get_recipe,
            commands::save_recipe,
            commands::delete_recipe,
            commands::run_recipe,
            commands::export_recipe,
            commands::import_recipe,
            commands::parse_recipe,
            commands::export_recipe_to_file,
            commands::import_recipe_from_file,
            commands::write_text_file,
            commands::get_settings,
            commands::save_settings,
            commands::app_paths,
            commands::ai_generate_recipe,
            commands::ai_test_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PasteCompiler");
}
