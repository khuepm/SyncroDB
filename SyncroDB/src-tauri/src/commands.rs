use crate::connection_manager::ConnectionManager;
use crate::credential_manager::CredentialManager;
use crate::migration_generator::MigrationGenerator;
use crate::models::{ConnectionTestResult, DatabaseConnection, DatabaseSchema, DatabaseType, SchemaComparison};
use crate::schema_analyzer::{
    MySQLAnalyzer, PostgreSQLAnalyzer, SQLServerAnalyzer, SQLiteAnalyzer, SchemaAnalyzer,
};
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


#[tauri::command]
pub async fn analyze_schema(
    connection_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<DatabaseSchema, String> {
    // Get connection details
    let credential_manager = state.credential_manager.lock().await;
    let connection = credential_manager.get_connection(&connection_id).await?;
    drop(credential_manager);

    // Get connection pool
    let pool = state
        .connection_manager
        .get_pool(&connection_id, &connection)
        .await?;

    // Route to appropriate analyzer based on database type
    let mut schema = match connection.db_type {
        DatabaseType::PostgreSQL => {
            let analyzer = PostgreSQLAnalyzer::new();
            analyzer.analyze_schema(&pool, &connection.database).await?
        }
        DatabaseType::MySQL => {
            let analyzer = MySQLAnalyzer::new();
            analyzer.analyze_schema(&pool, &connection.database).await?
        }
        DatabaseType::SQLite => {
            let analyzer = SQLiteAnalyzer::new();
            analyzer.analyze_schema(&pool, &connection.database).await?
        }
        DatabaseType::SQLServer => {
            let analyzer = SQLServerAnalyzer::new();
            analyzer.analyze_schema(&pool, &connection.database).await?
        }
    };

    // Set the connection_id in the schema
    schema.connection_id = connection_id;

    Ok(schema)
}

#[tauri::command]
pub async fn compare_schemas(
    source_id: String,
    target_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<crate::models::SchemaComparison, String> {
    // Analyze both schemas
    let source_schema = analyze_schema(source_id.clone(), state.clone()).await?;
    let target_schema = analyze_schema(target_id.clone(), state.clone()).await?;

    // Run diff engine comparison
    let diff_engine = crate::diff_engine::DiffEngine::new();
    let mut differences = diff_engine.compare_schemas(&source_schema, &target_schema)?;

    // Mark destructive operations
    crate::diff_engine::DiffEngine::mark_destructive_operations(&mut differences);

    // Generate summary statistics
    let summary = crate::diff_engine::DiffEngine::generate_summary(&differences);

    Ok(crate::models::SchemaComparison {
        source_id,
        target_id,
        differences,
        summary,
        compared_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
pub async fn generate_migration_script(
    target_connection_id: String,
    comparison: SchemaComparison,
    selected_operation_ids: Vec<String>,
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    // Get target connection to determine database type
    let credential_manager = state.credential_manager.lock().await;
    let target_connection = credential_manager.get_connection(&target_connection_id).await?;
    drop(credential_manager);

    // Create migration generator for the target database type
    let generator = MigrationGenerator::new(target_connection.db_type);

    // Generate the migration script
    let script = generator.generate_script(&comparison.differences, &selected_operation_ids)?;

    Ok(script)
}
