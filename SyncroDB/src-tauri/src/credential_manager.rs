use crate::error::{Result, SyncroDbError};
use crate::models::DatabaseConnection;
use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, AES_256_GCM};
use ring::error::Unspecified;
use ring::rand::{SecureRandom, SystemRandom};
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;

const NONCE_LEN: usize = 12;

// Simple nonce sequence that generates random nonces
struct RandomNonceSequence {
    rng: SystemRandom,
}

impl RandomNonceSequence {
    fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
        }
    }
}

impl NonceSequence for RandomNonceSequence {
    fn advance(&mut self) -> core::result::Result<Nonce, Unspecified> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        self.rng.fill(&mut nonce_bytes).map_err(|_| Unspecified)?;
        Nonce::try_assume_unique_for_key(&nonce_bytes)
    }
}

pub struct CredentialManager {
    pool: SqlitePool,
    encryption_key: Vec<u8>,
    rng: SystemRandom,
}

impl CredentialManager {
    pub async fn new() -> Result<Self> {
        let app_dir = Self::get_app_data_dir()?;
        std::fs::create_dir_all(&app_dir)?;
        
        let db_path = app_dir.join("connections.db");
        let db_url = format!("sqlite://{}", db_path.display());
        
        let pool = SqlitePool::connect(&db_url).await?;
        
        // Create connections table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS connections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                db_type TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                database TEXT NOT NULL,
                username TEXT NOT NULL,
                password_encrypted BLOB NOT NULL,
                ssl BOOLEAN NOT NULL,
                ssl_config TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;
        
        let encryption_key = Self::get_or_create_encryption_key(&app_dir)?;
        
        Ok(Self {
            pool,
            encryption_key,
            rng: SystemRandom::new(),
        })
    }

    // For testing: create with custom key
    #[cfg(test)]
    pub async fn new_with_key(db_url: &str, encryption_key: Vec<u8>) -> Result<Self> {
        let pool = SqlitePool::connect(db_url).await?;
        
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS connections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                db_type TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                database TEXT NOT NULL,
                username TEXT NOT NULL,
                password_encrypted BLOB NOT NULL,
                ssl BOOLEAN NOT NULL,
                ssl_config TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;
        
        Ok(Self {
            pool,
            encryption_key,
            rng: SystemRandom::new(),
        })
    }

    pub async fn save_connection(&self, connection: &DatabaseConnection) -> Result<()> {
        let encrypted_password = self.encrypt_password(&connection.password)?;
        let ssl_config_json = connection.ssl_config.as_ref()
            .map(|c| serde_json::to_string(c))
            .transpose()?;
        
        sqlx::query(
            r#"
            INSERT INTO connections (
                id, name, db_type, host, port, database, username,
                password_encrypted, ssl, ssl_config, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                db_type = excluded.db_type,
                host = excluded.host,
                port = excluded.port,
                database = excluded.database,
                username = excluded.username,
                password_encrypted = excluded.password_encrypted,
                ssl = excluded.ssl,
                ssl_config = excluded.ssl_config,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&connection.id)
        .bind(&connection.name)
        .bind(serde_json::to_string(&connection.db_type)?)
        .bind(&connection.host)
        .bind(connection.port as i64)
        .bind(&connection.database)
        .bind(&connection.username)
        .bind(&encrypted_password)
        .bind(connection.ssl)
        .bind(ssl_config_json)
        .bind(&connection.created_at)
        .bind(&connection.updated_at)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    pub async fn get_connection(&self, id: &str) -> Result<DatabaseConnection> {
        let row = sqlx::query(
            "SELECT id, name, db_type, host, port, database, username, password_encrypted, ssl, ssl_config, created_at, updated_at FROM connections WHERE id = ?"
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| SyncroDbError::NotFound(format!("Connection with id {} not found", id)))?;
        
        self.row_to_connection(row).await
    }

    pub async fn list_connections(&self) -> Result<Vec<DatabaseConnection>> {
        let rows = sqlx::query(
            "SELECT id, name, db_type, host, port, database, username, password_encrypted, ssl, ssl_config, created_at, updated_at FROM connections"
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut connections = Vec::new();
        for row in rows {
            connections.push(self.row_to_connection(row).await?);
        }
        
        Ok(connections)
    }

    pub async fn delete_connection(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM connections WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        if result.rows_affected() == 0 {
            return Err(SyncroDbError::NotFound(format!("Connection with id {} not found", id)));
        }
        
        Ok(())
    }

    pub fn encrypt_password(&self, password: &str) -> Result<Vec<u8>> {
        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.encryption_key)
            .map_err(|_| SyncroDbError::Encryption("Failed to create encryption key".to_string()))?;
        
        let mut sealing_key = SealingKey::new(unbound_key, RandomNonceSequence::new());
        
        let mut in_out = password.as_bytes().to_vec();
        
        // Generate nonce
        let mut nonce_bytes = [0u8; NONCE_LEN];
        self.rng.fill(&mut nonce_bytes)
            .map_err(|_| SyncroDbError::Encryption("Failed to generate nonce".to_string()))?;
        
        let _nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
            .map_err(|_| SyncroDbError::Encryption("Failed to create nonce".to_string()))?;
        
        let tag = sealing_key.seal_in_place_separate_tag(Aad::empty(), &mut in_out)
            .map_err(|_| SyncroDbError::Encryption("Failed to encrypt password".to_string()))?;
        
        // Prepend nonce to ciphertext + tag
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&in_out);
        result.extend_from_slice(tag.as_ref());
        
        Ok(result)
    }

    pub fn decrypt_password(&self, encrypted: &[u8]) -> Result<String> {
        if encrypted.len() < NONCE_LEN {
            return Err(SyncroDbError::Encryption("Invalid encrypted data".to_string()));
        }
        
        let (_nonce_bytes, ciphertext_and_tag) = encrypted.split_at(NONCE_LEN);
        
        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.encryption_key)
            .map_err(|_| SyncroDbError::Encryption("Failed to create decryption key".to_string()))?;
        
        let mut opening_key = OpeningKey::new(unbound_key, RandomNonceSequence::new());
        
        let mut in_out = ciphertext_and_tag.to_vec();
        
        let plaintext = opening_key.open_in_place(Aad::empty(), &mut in_out)
            .map_err(|_| SyncroDbError::Encryption("Failed to decrypt password".to_string()))?;
        
        String::from_utf8(plaintext.to_vec())
            .map_err(|_| SyncroDbError::Encryption("Invalid UTF-8 in decrypted password".to_string()))
    }

    async fn row_to_connection(&self, row: sqlx::sqlite::SqliteRow) -> Result<DatabaseConnection> {
        let encrypted_password: Vec<u8> = row.try_get("password_encrypted")?;
        let password = self.decrypt_password(&encrypted_password)?;
        
        let db_type_str: String = row.try_get("db_type")?;
        let db_type = serde_json::from_str(&db_type_str)?;
        
        let ssl_config_str: Option<String> = row.try_get("ssl_config")?;
        let ssl_config = ssl_config_str
            .map(|s| serde_json::from_str(&s))
            .transpose()?;
        
        Ok(DatabaseConnection {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            db_type,
            host: row.try_get("host")?,
            port: row.try_get::<i64, _>("port")? as u16,
            database: row.try_get("database")?,
            username: row.try_get("username")?,
            password,
            ssl: row.try_get("ssl")?,
            ssl_config,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    fn get_app_data_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| SyncroDbError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not find home directory",
            )))?;
        
        Ok(home.join(".syncrodb"))
    }

    fn get_or_create_encryption_key(app_dir: &PathBuf) -> Result<Vec<u8>> {
        let key_path = app_dir.join("encryption.key");
        
        if key_path.exists() {
            let key = std::fs::read(&key_path)?;
            if key.len() != 32 {
                return Err(SyncroDbError::Encryption("Invalid encryption key length".to_string()));
            }
            Ok(key)
        } else {
            // Generate new key
            let rng = SystemRandom::new();
            let mut key = vec![0u8; 32];
            rng.fill(&mut key)
                .map_err(|_| SyncroDbError::Encryption("Failed to generate encryption key".to_string()))?;
            
            std::fs::write(&key_path, &key)?;
            
            Ok(key)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Helper to generate arbitrary database connections
    fn arbitrary_connection() -> impl Strategy<Value = DatabaseConnection> {
        (
            "[a-z0-9]{8}",
            "[A-Za-z ]{5,20}",
            prop_oneof![
                Just(crate::models::DatabaseType::PostgreSQL),
                Just(crate::models::DatabaseType::MySQL),
                Just(crate::models::DatabaseType::SQLite),
            ],
            "[a-z]{5,15}",
            1024u16..65535u16,
            "[a-z]{5,15}",
            "[a-z]{5,15}",
            "[\\x20-\\x7E]{8,64}",
            any::<bool>(),
        ).prop_map(|(id, name, db_type, host, port, database, username, password, ssl)| {
            DatabaseConnection {
                id,
                name,
                db_type,
                host,
                port,
                database,
                username,
                password,
                ssl,
                ssl_config: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            }
        })
    }

    // Feature: syncrodb-schema-sync, Property 1: Connection Persistence Round-Trip
    // **Validates: Requirements 1.6**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        #[test]
        fn test_connection_persistence_roundtrip(
            connections in prop::collection::vec(arbitrary_connection(), 1..10)
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let rng = SystemRandom::new();
                let mut key = vec![0u8; 32];
                rng.fill(&mut key).unwrap();
                
                let manager = CredentialManager::new_with_key("sqlite::memory:", key).await.unwrap();
                
                // Save all connections
                for conn in &connections {
                    manager.save_connection(conn).await.unwrap();
                }
                
                // Load all connections back
                let loaded = manager.list_connections().await.unwrap();
                
                // Verify count matches
                prop_assert_eq!(connections.len(), loaded.len());
                
                // Verify each connection was persisted correctly
                for original in &connections {
                    let retrieved = manager.get_connection(&original.id).await.unwrap();
                    
                    prop_assert_eq!(original.id, retrieved.id);
                    prop_assert_eq!(original.name, retrieved.name);
                    prop_assert_eq!(original.host, retrieved.host);
                    prop_assert_eq!(original.port, retrieved.port);
                    prop_assert_eq!(original.database, retrieved.database);
                    prop_assert_eq!(original.username, retrieved.username);
                    prop_assert_eq!(original.password, retrieved.password);
                    prop_assert_eq!(original.ssl, retrieved.ssl);
                }
            });
        }
    }

    // Feature: syncrodb-schema-sync, Property 12: Credential Encryption and Security
    // **Validates: Requirements 7.1, 7.2**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        #[test]
        fn test_credential_encryption_roundtrip(password in "[\\x20-\\x7E]{8,64}") {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Generate a random encryption key
                let rng = SystemRandom::new();
                let mut key = vec![0u8; 32];
                rng.fill(&mut key).unwrap();
                
                let manager = CredentialManager::new_with_key("sqlite::memory:", key).await.unwrap();
                
                // Encrypt the password
                let encrypted = manager.encrypt_password(&password).unwrap();
                
                // Verify encrypted data is not the same as plaintext
                prop_assert_ne!(encrypted, password.as_bytes());
                
                // Verify encrypted data is longer (includes nonce and tag)
                prop_assert!(encrypted.len() > password.len());
                
                // Decrypt and verify we get the original password back
                let decrypted = manager.decrypt_password(&encrypted).unwrap();
                prop_assert_eq!(decrypted, password);
                
                // Verify encrypting the same password twice produces different ciphertext (due to random nonce)
                let encrypted2 = manager.encrypt_password(&password).unwrap();
                prop_assert_ne!(encrypted, encrypted2);
                
                // But both should decrypt to the same plaintext
                let decrypted2 = manager.decrypt_password(&encrypted2).unwrap();
                prop_assert_eq!(decrypted2, password);
            });
        }
        
        #[test]
        fn test_stored_credentials_never_plaintext(
            password in "[\\x20-\\x7E]{8,64}",
            id in "[a-z0-9]{8}",
            name in "[A-Za-z ]{5,20}",
            host in "[a-z]{5,15}",
            database in "[a-z]{5,15}",
            username in "[a-z]{5,15}",
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let rng = SystemRandom::new();
                let mut key = vec![0u8; 32];
                rng.fill(&mut key).unwrap();
                
                let manager = CredentialManager::new_with_key("sqlite::memory:", key).await.unwrap();
                
                let connection = DatabaseConnection {
                    id: id.clone(),
                    name,
                    db_type: crate::models::DatabaseType::PostgreSQL,
                    host,
                    port: 5432,
                    database,
                    username,
                    password: password.clone(),
                    ssl: false,
                    ssl_config: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                };
                
                manager.save_connection(&connection).await.unwrap();
                
                // Query the raw database to verify password is encrypted
                let row: (Vec<u8>,) = sqlx::query_as(
                    "SELECT password_encrypted FROM connections WHERE id = ?"
                )
                .bind(&id)
                .fetch_one(&manager.pool)
                .await
                .unwrap();
                
                let stored_encrypted = row.0;
                
                // Verify stored data is NOT the plaintext password
                prop_assert_ne!(stored_encrypted, password.as_bytes());
                
                // Verify we can retrieve and decrypt the connection
                let retrieved = manager.get_connection(&id).await.unwrap();
                prop_assert_eq!(retrieved.password, password);
            });
        }
    }
}


    // Unit tests for connection CRUD operations
    #[tokio::test]
    async fn test_add_and_get_connection() {
        let rng = SystemRandom::new();
        let mut key = vec![0u8; 32];
        rng.fill(&mut key).unwrap();
        
        let manager = CredentialManager::new_with_key("sqlite::memory:", key).await.unwrap();
        
        let connection = DatabaseConnection {
            id: "test123".to_string(),
            name: "Test DB".to_string(),
            db_type: crate::models::DatabaseType::PostgreSQL,
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "testuser".to_string(),
            password: "testpass123".to_string(),
            ssl: false,
            ssl_config: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        
        manager.save_connection(&connection).await.unwrap();
        let retrieved = manager.get_connection("test123").await.unwrap();
        
        assert_eq!(connection.id, retrieved.id);
        assert_eq!(connection.name, retrieved.name);
        assert_eq!(connection.password, retrieved.password);
    }
    
    #[tokio::test]
    async fn test_update_connection() {
        let rng = SystemRandom::new();
        let mut key = vec![0u8; 32];
        rng.fill(&mut key).unwrap();
        
        let manager = CredentialManager::new_with_key("sqlite::memory:", key).await.unwrap();
        
        let mut connection = DatabaseConnection {
            id: "test456".to_string(),
            name: "Original Name".to_string(),
            db_type: crate::models::DatabaseType::MySQL,
            host: "localhost".to_string(),
            port: 3306,
            database: "testdb".to_string(),
            username: "testuser".to_string(),
            password: "password123".to_string(),
            ssl: false,
            ssl_config: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        
        manager.save_connection(&connection).await.unwrap();
        
        // Update the connection
        connection.name = "Updated Name".to_string();
        connection.password = "newpassword456".to_string();
        manager.save_connection(&connection).await.unwrap();
        
        let retrieved = manager.get_connection("test456").await.unwrap();
        assert_eq!("Updated Name", retrieved.name);
        assert_eq!("newpassword456", retrieved.password);
    }
    
    #[tokio::test]
    async fn test_delete_connection() {
        let rng = SystemRandom::new();
        let mut key = vec![0u8; 32];
        rng.fill(&mut key).unwrap();
        
        let manager = CredentialManager::new_with_key("sqlite::memory:", key).await.unwrap();
        
        let connection = DatabaseConnection {
            id: "test789".to_string(),
            name: "To Delete".to_string(),
            db_type: crate::models::DatabaseType::SQLite,
            host: "".to_string(),
            port: 0,
            database: "test.db".to_string(),
            username: "".to_string(),
            password: "".to_string(),
            ssl: false,
            ssl_config: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        
        manager.save_connection(&connection).await.unwrap();
        manager.delete_connection("test789").await.unwrap();
        
        let result = manager.get_connection("test789").await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_list_connections() {
        let rng = SystemRandom::new();
        let mut key = vec![0u8; 32];
        rng.fill(&mut key).unwrap();
        
        let manager = CredentialManager::new_with_key("sqlite::memory:", key).await.unwrap();
        
        let conn1 = DatabaseConnection {
            id: "conn1".to_string(),
            name: "Connection 1".to_string(),
            db_type: crate::models::DatabaseType::PostgreSQL,
            host: "localhost".to_string(),
            port: 5432,
            database: "db1".to_string(),
            username: "user1".to_string(),
            password: "pass1".to_string(),
            ssl: false,
            ssl_config: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        
        let conn2 = DatabaseConnection {
            id: "conn2".to_string(),
            name: "Connection 2".to_string(),
            db_type: crate::models::DatabaseType::MySQL,
            host: "localhost".to_string(),
            port: 3306,
            database: "db2".to_string(),
            username: "user2".to_string(),
            password: "pass2".to_string(),
            ssl: true,
            ssl_config: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        
        manager.save_connection(&conn1).await.unwrap();
        manager.save_connection(&conn2).await.unwrap();
        
        let connections = manager.list_connections().await.unwrap();
        assert_eq!(2, connections.len());
    }
    
    #[tokio::test]
    async fn test_ssl_configuration() {
        let rng = SystemRandom::new();
        let mut key = vec![0u8; 32];
        rng.fill(&mut key).unwrap();
        
        let manager = CredentialManager::new_with_key("sqlite::memory:", key).await.unwrap();
        
        let ssl_config = crate::models::SSLConfig {
            mode: "require".to_string(),
            ca: Some("/path/to/ca.crt".to_string()),
            cert: Some("/path/to/client.crt".to_string()),
            key: Some("/path/to/client.key".to_string()),
        };
        
        let connection = DatabaseConnection {
            id: "ssl_test".to_string(),
            name: "SSL Connection".to_string(),
            db_type: crate::models::DatabaseType::PostgreSQL,
            host: "secure.example.com".to_string(),
            port: 5432,
            database: "securedb".to_string(),
            username: "secureuser".to_string(),
            password: "securepass".to_string(),
            ssl: true,
            ssl_config: Some(ssl_config),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        
        manager.save_connection(&connection).await.unwrap();
        let retrieved = manager.get_connection("ssl_test").await.unwrap();
        
        assert!(retrieved.ssl);
        assert!(retrieved.ssl_config.is_some());
        let retrieved_ssl = retrieved.ssl_config.unwrap();
        assert_eq!("require", retrieved_ssl.mode);
        assert_eq!(Some("/path/to/ca.crt".to_string()), retrieved_ssl.ca);
    }
