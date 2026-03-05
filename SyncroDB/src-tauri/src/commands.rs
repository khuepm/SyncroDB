use crate::connection_manager::ConnectionManager;
use crate::credential_manager::CredentialManager;
use crate::error::Result;
use crate::models::{ConnectionTestResult, DatabaseConnection};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

pub struct AppState {
    pub credential_manager: Arc<Mutex<CredentialManager>>,
    pub connection_manager: Arc<ConnectionManager>,
}

#[tauri::command]
pub async fn add_connection(
    connection: DatabaseConnection,
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    let credential_manager = state.credential_manager.lock().await;
    credential_manager.save_connection(&connection).await?;
    Ok(connection.id.clone())
}

#[tauri::command]
pub async fn update_connection(
    id: String,
    connection: DatabaseConnection,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    if id != connection.id {
        return Err("Connection ID mismatch".to_string());
    }
    
    let credential_manager = state.credential_manager.lock().await;
    credential_manager.save_connection(&connection).await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_connection(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<(), String> {
    let credential_manager = state.credential_manager.lock().await;
    credential_manager.delete_connection(&id).await?;
    
    // Also close any active connection pool
    state.connection_manager.close_pool(&id).await;
    
    Ok(())
}

#[tauri::command]
pub async fn list_connections(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<DatabaseConnection>, String> {
    let credential_manager = state.credential_manager.lock().await;
    let connections = credential_manager.list_connections().await?;
    Ok(connections)
}

#[tauri::command]
pub async fn test_connection(
    id: String,
    state: State<'_, AppState>,
) -> std::result::Result<ConnectionTestResult, String> {
    let credential_manager = state.credential_manager.lock().await;
    let connection = credential_manager.get_connection(&id).await?;
    drop(credential_manager);
    
    let result = state.connection_manager.test_connection(&connection).await?;
    Ok(result)
}
