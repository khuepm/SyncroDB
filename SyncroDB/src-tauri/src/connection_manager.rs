use crate::error::{Result, SyncroDbError};
use crate::models::{ConnectionTestResult, DatabaseConnection, DatabaseType};
use sqlx::{Any, AnyPool, Pool};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct ConnectionManager {
    pools: Arc<RwLock<HashMap<String, Pool<Any>>>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn test_connection(&self, connection: &DatabaseConnection) -> Result<ConnectionTestResult> {
        let start = Instant::now();
        
        let connection_string = self.build_connection_string(connection)?;
        
        match AnyPool::connect(&connection_string).await {
            Ok(pool) => {
                let latency = start.elapsed().as_millis() as u64;
                
                // Get server version
                let version = self.get_server_version(&pool, &connection.db_type).await.ok();
                
                pool.close().await;
                
                Ok(ConnectionTestResult {
                    success: true,
                    message: "Connection successful".to_string(),
                    latency: Some(latency),
                    server_version: version,
                })
            }
            Err(e) => Ok(ConnectionTestResult {
                success: false,
                message: format!("Connection failed: {}", e),
                latency: None,
                server_version: None,
            }),
        }
    }

    pub async fn get_pool(&self, connection_id: &str, connection: &DatabaseConnection) -> Result<Pool<Any>> {
        let pools = self.pools.read().await;
        
        if let Some(pool) = pools.get(connection_id) {
            return Ok(pool.clone());
        }
        
        drop(pools);
        
        // Create new pool
        let connection_string = self.build_connection_string(connection)?;
        let pool = AnyPool::connect(&connection_string)
            .await
            .map_err(|e| SyncroDbError::Connection(format!("Failed to create connection pool: {}", e)))?;
        
        let mut pools = self.pools.write().await;
        pools.insert(connection_id.to_string(), pool.clone());
        
        Ok(pool)
    }

    pub async fn close_pool(&self, connection_id: &str) {
        let mut pools = self.pools.write().await;
        if let Some(pool) = pools.remove(connection_id) {
            pool.close().await;
        }
    }

    fn build_connection_string(&self, connection: &DatabaseConnection) -> Result<String> {
        let ssl_param = if connection.ssl {
            match connection.db_type {
                DatabaseType::PostgreSQL => "?sslmode=require",
                DatabaseType::MySQL => "?ssl-mode=REQUIRED",
                DatabaseType::SQLServer => ";encrypt=true",
                DatabaseType::SQLite => "",
            }
        } else {
            ""
        };

        let conn_str = match connection.db_type {
            DatabaseType::PostgreSQL => {
                format!(
                    "postgresql://{}:{}@{}:{}/{}{}",
                    connection.username,
                    connection.password,
                    connection.host,
                    connection.port,
                    connection.database,
                    ssl_param
                )
            }
            DatabaseType::MySQL => {
                format!(
                    "mysql://{}:{}@{}:{}/{}{}",
                    connection.username,
                    connection.password,
                    connection.host,
                    connection.port,
                    connection.database,
                    ssl_param
                )
            }
            DatabaseType::SQLServer => {
                format!(
                    "mssql://{}:{}@{}:{}/{}{}",
                    connection.username,
                    connection.password,
                    connection.host,
                    connection.port,
                    connection.database,
                    ssl_param
                )
            }
            DatabaseType::SQLite => {
                format!("sqlite://{}", connection.database)
            }
        };

        Ok(conn_str)
    }

    async fn get_server_version(&self, pool: &Pool<Any>, db_type: &DatabaseType) -> Result<String> {
        let query = match db_type {
            DatabaseType::PostgreSQL => "SELECT version()",
            DatabaseType::MySQL => "SELECT VERSION()",
            DatabaseType::SQLServer => "SELECT @@VERSION",
            DatabaseType::SQLite => "SELECT sqlite_version()",
        };

        let row: (String,) = sqlx::query_as(query)
            .fetch_one(pool)
            .await
            .map_err(|e| SyncroDbError::Database(e))?;

        Ok(row.0)
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
