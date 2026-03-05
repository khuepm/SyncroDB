mod error;
mod models;
mod connection_manager;
mod schema_analyzer;
mod diff_engine;
mod migration_generator;
mod executor;
mod credential_manager;
mod commands;

#[cfg(test)]
mod schema_analyzer_tests;

#[cfg(test)]
mod diff_engine_tests;

pub use error::{Result, SyncroDbError};
pub use models::*;

use commands::AppState;
use connection_manager::ConnectionManager;
use credential_manager::CredentialManager;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let credential_manager = tauri::async_runtime::block_on(async {
                CredentialManager::new().await.expect("Failed to initialize credential manager")
            });
            
            let app_state = AppState {
                credential_manager: Arc::new(Mutex::new(credential_manager)),
                connection_manager: Arc::new(ConnectionManager::new()),
            };
            
            app.manage(app_state);
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_connection,
            commands::update_connection,
            commands::delete_connection,
            commands::list_connections,
            commands::test_connection,
            commands::analyze_schema,
            commands::compare_schemas,
            commands::generate_migration_script,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
