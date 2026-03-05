#[cfg(test)]
mod tests {
    use crate::schema_analyzer::{SchemaAnalyzer, SQLiteAnalyzer};
    use sqlx::{SqlitePool, AnyPool};

    // Feature: syncrodb-schema-sync, Property 3: Comprehensive Schema Extraction
    // **Validates: Requirements 2.1, 2.2, 8.1, 8.2, 8.3, 8.4, 8.5, 8.6**
    //
    // For any database connection, the schema analyzer should extract all supported schema objects
    // including tables (with columns, data types, nullability), primary keys, foreign keys,
    // unique constraints, check constraints, indexes (all types), views, stored procedures,
    // functions, triggers, sequences, and database-specific objects, with warnings logged for
    // unsupported objects.
    //
    // Note: These tests use SQLite directly with conversion to AnyPool. The actual implementation
    // uses AnyPool through the connection manager which handles database-specific pool creation.

    #[tokio::test]
    async fn test_sqlite_comprehensive_schema_extraction() {
        // Create an in-memory SQLite database for testing
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");

        // Create a comprehensive test schema
        sqlx::query(
            r#"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                email TEXT NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create users table");

        sqlx::query(
            r#"
            CREATE TABLE posts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                content TEXT,
                published BOOLEAN DEFAULT 0,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create posts table");

        sqlx::query("CREATE INDEX idx_posts_user_id ON posts(user_id)")
            .execute(&pool)
            .await
            .expect("Failed to create index");

        sqlx::query("CREATE VIEW published_posts AS SELECT * FROM posts WHERE published = 1")
            .execute(&pool)
            .await
            .expect("Failed to create view");

        sqlx::query(
            r#"
            CREATE TRIGGER update_timestamp
            AFTER UPDATE ON posts
            BEGIN
                UPDATE posts SET created_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
            END
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create trigger");

        // Convert to AnyPool using connection string
        let any_pool = AnyPool::connect("sqlite::memory:")
            .await
            .expect("Failed to create AnyPool");

        // Recreate schema in AnyPool (since it's a new connection)
        sqlx::query(
            r#"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                email TEXT NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&any_pool)
        .await
        .expect("Failed to create users table");

        sqlx::query(
            r#"
            CREATE TABLE posts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                content TEXT,
                published BOOLEAN DEFAULT 0,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&any_pool)
        .await
        .expect("Failed to create posts table");

        sqlx::query("CREATE INDEX idx_posts_user_id ON posts(user_id)")
            .execute(&any_pool)
            .await
            .expect("Failed to create index");

        sqlx::query("CREATE VIEW published_posts AS SELECT * FROM posts WHERE published = 1")
            .execute(&any_pool)
            .await
            .expect("Failed to create view");

        sqlx::query(
            r#"
            CREATE TRIGGER update_timestamp
            AFTER UPDATE ON posts
            BEGIN
                UPDATE posts SET created_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
            END
            "#,
        )
        .execute(&any_pool)
        .await
        .expect("Failed to create trigger");

        // Analyze the schema
        let analyzer = SQLiteAnalyzer::new();
        let schema = analyzer
            .analyze_schema(&any_pool, "test_db")
            .await
            .expect("Schema analysis failed");

        // Verify comprehensive extraction
        
        // 8.1: Tables with columns, data types, and nullability
        assert_eq!(schema.tables.len(), 2, "Should extract 2 tables");
        
        let users_table = schema.tables.iter().find(|t| t.name == "users").expect("users table not found");
        assert_eq!(users_table.columns.len(), 4, "users table should have 4 columns");
        
        let id_column = users_table.columns.iter().find(|c| c.name == "id").expect("id column not found");
        assert_eq!(id_column.data_type.to_uppercase(), "INTEGER");
        assert!(!id_column.nullable, "id should not be nullable");
        assert!(id_column.auto_increment, "id should be auto-increment");
        
        let username_column = users_table.columns.iter().find(|c| c.name == "username").expect("username column not found");
        assert!(!username_column.nullable, "username should not be nullable");
        
        // 8.2: Primary keys
        assert!(users_table.primary_key.is_some(), "users table should have primary key");
        let pk = users_table.primary_key.as_ref().unwrap();
        assert_eq!(pk.columns, vec!["id"], "Primary key should be on id column");
        
        // 8.2: Foreign keys
        let posts_table = schema.tables.iter().find(|t| t.name == "posts").expect("posts table not found");
        assert_eq!(posts_table.foreign_keys.len(), 1, "posts table should have 1 foreign key");
        let fk = &posts_table.foreign_keys[0];
        assert_eq!(fk.columns, vec!["user_id"]);
        assert_eq!(fk.referenced_table, "users");
        assert_eq!(fk.referenced_columns, vec!["id"]);
        assert_eq!(fk.on_delete, "CASCADE");
        
        // 8.3: Indexes
        assert!(!posts_table.indexes.is_empty(), "posts table should have indexes");
        let has_user_id_index = posts_table.indexes.iter().any(|idx| idx.name == "idx_posts_user_id");
        assert!(has_user_id_index, "Should have idx_posts_user_id index");
        
        // 8.4: Views
        assert_eq!(schema.views.len(), 1, "Should extract 1 view");
        let view = &schema.views[0];
        assert_eq!(view.name, "published_posts");
        assert!(view.definition.contains("published"), "View definition should contain 'published'");
        
        // 8.5: Triggers
        assert_eq!(schema.triggers.len(), 1, "Should extract 1 trigger");
        let trigger = &schema.triggers[0];
        assert_eq!(trigger.name, "update_timestamp");
        assert_eq!(trigger.table, "posts");
        
        // 8.6: Complete DatabaseSchema structure
        assert!(!schema.database_name.is_empty(), "Database name should be set");
        assert!(!schema.analyzed_at.is_empty(), "analyzed_at timestamp should be set");
        
        // SQLite doesn't support procedures, functions, or sequences
        assert_eq!(schema.procedures.len(), 0, "SQLite should have no procedures");
        assert_eq!(schema.functions.len(), 0, "SQLite should have no functions");
        assert_eq!(schema.sequences.len(), 0, "SQLite should have no sequences");
    }

    #[tokio::test]
    async fn test_schema_extraction_handles_empty_database() {
        let any_pool = AnyPool::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");

        let analyzer = SQLiteAnalyzer::new();
        let schema = analyzer
            .analyze_schema(&any_pool, "empty_db")
            .await
            .expect("Schema analysis should succeed on empty database");

        assert_eq!(schema.tables.len(), 0, "Empty database should have no tables");
        assert_eq!(schema.views.len(), 0, "Empty database should have no views");
        assert_eq!(schema.triggers.len(), 0, "Empty database should have no triggers");
        assert!(!schema.analyzed_at.is_empty(), "analyzed_at should still be set");
    }

    #[tokio::test]
    async fn test_schema_extraction_handles_complex_data_types() {
        let any_pool = AnyPool::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");

        sqlx::query(
            r#"
            CREATE TABLE complex_types (
                id INTEGER PRIMARY KEY,
                text_col TEXT,
                int_col INTEGER,
                real_col REAL,
                blob_col BLOB,
                null_col TEXT
            )
            "#,
        )
        .execute(&any_pool)
        .await
        .expect("Failed to create table");

        let analyzer = SQLiteAnalyzer::new();
        let schema = analyzer
            .analyze_schema(&any_pool, "test_db")
            .await
            .expect("Schema analysis failed");

        let table = &schema.tables[0];
        assert_eq!(table.columns.len(), 6, "Should extract all columns with different types");
        
        // Verify different data types are captured
        let data_types: Vec<String> = table.columns.iter().map(|c| c.data_type.to_uppercase()).collect();
        assert!(data_types.contains(&"TEXT".to_string()), "Should have TEXT type");
        assert!(data_types.contains(&"INTEGER".to_string()), "Should have INTEGER type");
        assert!(data_types.contains(&"REAL".to_string()), "Should have REAL type");
        assert!(data_types.contains(&"BLOB".to_string()), "Should have BLOB type");
    }

    #[tokio::test]
    async fn test_schema_extraction_handles_multiple_constraints() {
        let any_pool = AnyPool::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database");

        sqlx::query(
            r#"
            CREATE TABLE products (
                id INTEGER PRIMARY KEY,
                sku TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                price REAL NOT NULL,
                category_id INTEGER,
                supplier_id INTEGER,
                FOREIGN KEY (category_id) REFERENCES categories(id),
                FOREIGN KEY (supplier_id) REFERENCES suppliers(id)
            )
            "#,
        )
        .execute(&any_pool)
        .await
        .expect("Failed to create table");

        sqlx::query("CREATE INDEX idx_products_category ON products(category_id)")
            .execute(&any_pool)
            .await
            .expect("Failed to create index");

        sqlx::query("CREATE INDEX idx_products_supplier ON products(supplier_id)")
            .execute(&any_pool)
            .await
            .expect("Failed to create index");

        let analyzer = SQLiteAnalyzer::new();
        let schema = analyzer
            .analyze_schema(&any_pool, "test_db")
            .await
            .expect("Schema analysis failed");

        let table = &schema.tables[0];
        
        // Verify multiple foreign keys
        assert_eq!(table.foreign_keys.len(), 2, "Should extract 2 foreign keys");
        
        // Verify multiple indexes
        assert!(table.indexes.len() >= 2, "Should have at least 2 indexes");
        
        // Verify unique constraint (captured as index in SQLite)
        let has_unique_index = table.indexes.iter().any(|idx| idx.unique && idx.columns.contains(&"sku".to_string()));
        assert!(has_unique_index, "Should have unique index on sku");
    }
}
