use crate::error::{Result, SyncroDbError};
use crate::models::*;
use async_trait::async_trait;
use sqlx::{AnyPool, Row};
use std::collections::HashMap;

#[async_trait]
pub trait SchemaAnalyzer {
    async fn analyze_schema(&self, pool: &AnyPool, database_name: &str) -> Result<DatabaseSchema>;
}

pub struct PostgreSQLAnalyzer;
pub struct MySQLAnalyzer;
pub struct SQLiteAnalyzer;
pub struct SQLServerAnalyzer;

impl PostgreSQLAnalyzer {
    pub fn new() -> Self {
        Self
    }

    async fn get_tables(&self, pool: &AnyPool) -> Result<Vec<Table>> {
        let query = r#"
            SELECT 
                t.table_schema,
                t.table_name
            FROM information_schema.tables t
            WHERE t.table_schema NOT IN ('pg_catalog', 'information_schema')
                AND t.table_type = 'BASE TABLE'
            ORDER BY t.table_schema, t.table_name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await
            .map_err(|e| SyncroDbError::SchemaAnalysis(format!("Failed to fetch tables: {}", e)))?;

        let mut tables = Vec::new();
        for row in rows {
            let schema: String = row.try_get("table_schema")?;
            let name: String = row.try_get("table_name")?;

            let columns = self.get_columns(pool, &schema, &name).await?;
            let primary_key = self.get_primary_key(pool, &schema, &name).await?;
            let foreign_keys = self.get_foreign_keys(pool, &schema, &name).await?;
            let unique_constraints = self.get_unique_constraints(pool, &schema, &name).await?;
            let check_constraints = self.get_check_constraints(pool, &schema, &name).await?;
            let indexes = self.get_indexes(pool, &schema, &name).await?;
            let row_count = self.get_row_count(pool, &schema, &name).await.ok();

            tables.push(Table {
                name,
                schema,
                columns,
                primary_key,
                foreign_keys,
                unique_constraints,
                check_constraints,
                indexes,
                row_count,
            });
        }

        Ok(tables)
    }

    async fn get_columns(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<Column>> {
        let query = r#"
            SELECT 
                column_name,
                data_type,
                is_nullable,
                column_default,
                ordinal_position,
                col_description((table_schema||'.'||table_name)::regclass::oid, ordinal_position) as comment
            FROM information_schema.columns
            WHERE table_schema = $1 AND table_name = $2
            ORDER BY ordinal_position
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut columns = Vec::new();
        for row in rows {
            let name: String = row.try_get("column_name")?;
            let data_type: String = row.try_get("data_type")?;
            let is_nullable: String = row.try_get("is_nullable")?;
            let default_value: Option<String> = row.try_get("column_default").ok();
            let ordinal_position: i32 = row.try_get("ordinal_position")?;
            let comment: Option<String> = row.try_get("comment").ok();

            let auto_increment = default_value
                .as_ref()
                .map(|d| d.contains("nextval"))
                .unwrap_or(false);

            columns.push(Column {
                name,
                data_type,
                nullable: is_nullable == "YES",
                default_value,
                auto_increment,
                comment,
                ordinal_position,
            });
        }

        Ok(columns)
    }

    async fn get_primary_key(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Option<PrimaryKey>> {
        let query = r#"
            SELECT
                tc.constraint_name,
                array_agg(kcu.column_name ORDER BY kcu.ordinal_position) as columns
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'PRIMARY KEY'
                AND tc.table_schema = $1
                AND tc.table_name = $2
            GROUP BY tc.constraint_name
        "#;

        let row = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_optional(pool)
            .await?;

        if let Some(row) = row {
            let name: String = row.try_get("constraint_name")?;
            let columns: Vec<String> = row.try_get("columns")?;

            Ok(Some(PrimaryKey { name, columns }))
        } else {
            Ok(None)
        }
    }

    async fn get_foreign_keys(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<ForeignKey>> {
        let query = r#"
            SELECT
                tc.constraint_name,
                array_agg(kcu.column_name ORDER BY kcu.ordinal_position) as columns,
                ccu.table_name AS referenced_table,
                array_agg(ccu.column_name ORDER BY kcu.ordinal_position) as referenced_columns,
                rc.update_rule,
                rc.delete_rule
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage ccu
                ON tc.constraint_name = ccu.constraint_name
            JOIN information_schema.referential_constraints rc
                ON tc.constraint_name = rc.constraint_name
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND tc.table_schema = $1
                AND tc.table_name = $2
            GROUP BY tc.constraint_name, ccu.table_name, rc.update_rule, rc.delete_rule
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut foreign_keys = Vec::new();
        for row in rows {
            let name: String = row.try_get("constraint_name")?;
            let columns: Vec<String> = row.try_get("columns")?;
            let referenced_table: String = row.try_get("referenced_table")?;
            let referenced_columns: Vec<String> = row.try_get("referenced_columns")?;
            let on_update: String = row.try_get("update_rule")?;
            let on_delete: String = row.try_get("delete_rule")?;

            foreign_keys.push(ForeignKey {
                name,
                columns,
                referenced_table,
                referenced_columns,
                on_update,
                on_delete,
            });
        }

        Ok(foreign_keys)
    }

    async fn get_unique_constraints(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<UniqueConstraint>> {
        let query = r#"
            SELECT
                tc.constraint_name,
                array_agg(kcu.column_name ORDER BY kcu.ordinal_position) as columns
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'UNIQUE'
                AND tc.table_schema = $1
                AND tc.table_name = $2
            GROUP BY tc.constraint_name
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut constraints = Vec::new();
        for row in rows {
            let name: String = row.try_get("constraint_name")?;
            let columns: Vec<String> = row.try_get("columns")?;

            constraints.push(UniqueConstraint { name, columns });
        }

        Ok(constraints)
    }

    async fn get_check_constraints(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<CheckConstraint>> {
        let query = r#"
            SELECT
                tc.constraint_name,
                cc.check_clause
            FROM information_schema.table_constraints tc
            JOIN information_schema.check_constraints cc
                ON tc.constraint_name = cc.constraint_name
            WHERE tc.constraint_type = 'CHECK'
                AND tc.table_schema = $1
                AND tc.table_name = $2
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut constraints = Vec::new();
        for row in rows {
            let name: String = row.try_get("constraint_name")?;
            let expression: String = row.try_get("check_clause")?;

            constraints.push(CheckConstraint { name, expression });
        }

        Ok(constraints)
    }

    async fn get_indexes(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<Index>> {
        let query = r#"
            SELECT
                i.relname as index_name,
                array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) as columns,
                ix.indisunique as is_unique,
                am.amname as index_type,
                pg_get_expr(ix.indpred, ix.indrelid) as condition
            FROM pg_class t
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_am am ON i.relam = am.oid
            JOIN pg_namespace n ON t.relnamespace = n.oid
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            WHERE n.nspname = $1
                AND t.relname = $2
                AND NOT ix.indisprimary
            GROUP BY i.relname, ix.indisunique, am.amname, ix.indpred, ix.indrelid
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut indexes = Vec::new();
        for row in rows {
            let name: String = row.try_get("index_name")?;
            let columns: Vec<String> = row.try_get("columns")?;
            let unique: bool = row.try_get("is_unique")?;
            let index_type: String = row.try_get("index_type")?;
            let condition: Option<String> = row.try_get("condition").ok();

            indexes.push(Index {
                name,
                columns,
                unique,
                index_type,
                partial: condition.is_some(),
                condition,
            });
        }

        Ok(indexes)
    }

    async fn get_row_count(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<i64> {
        let query = format!(
            "SELECT COUNT(*) as count FROM {}.{}",
            schema, table
        );

        let row = sqlx::query(&query)
            .fetch_one(pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count)
    }

    async fn get_views(&self, pool: &AnyPool) -> Result<Vec<View>> {
        let query = r#"
            SELECT
                table_schema,
                table_name,
                view_definition
            FROM information_schema.views
            WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY table_schema, table_name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut views = Vec::new();
        for row in rows {
            let schema: String = row.try_get("table_schema")?;
            let name: String = row.try_get("table_name")?;
            let definition: String = row.try_get("view_definition")?;

            views.push(View {
                name,
                schema,
                definition,
            });
        }

        Ok(views)
    }

    async fn get_procedures(&self, pool: &AnyPool) -> Result<Vec<StoredProcedure>> {
        let query = r#"
            SELECT
                n.nspname as schema_name,
                p.proname as procedure_name,
                pg_get_functiondef(p.oid) as definition
            FROM pg_proc p
            JOIN pg_namespace n ON p.pronamespace = n.oid
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
                AND p.prokind = 'p'
            ORDER BY n.nspname, p.proname
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut procedures = Vec::new();
        for row in rows {
            let schema: String = row.try_get("schema_name")?;
            let name: String = row.try_get("procedure_name")?;
            let definition: String = row.try_get("definition")?;

            procedures.push(StoredProcedure {
                name,
                schema,
                parameters: Vec::new(), // Parameters are in the definition
                definition,
            });
        }

        Ok(procedures)
    }

    async fn get_functions(&self, pool: &AnyPool) -> Result<Vec<Function>> {
        let query = r#"
            SELECT
                n.nspname as schema_name,
                p.proname as function_name,
                pg_get_function_result(p.oid) as return_type,
                pg_get_functiondef(p.oid) as definition
            FROM pg_proc p
            JOIN pg_namespace n ON p.pronamespace = n.oid
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
                AND p.prokind = 'f'
            ORDER BY n.nspname, p.proname
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut functions = Vec::new();
        for row in rows {
            let schema: String = row.try_get("schema_name")?;
            let name: String = row.try_get("function_name")?;
            let return_type: String = row.try_get("return_type")?;
            let definition: String = row.try_get("definition")?;

            functions.push(Function {
                name,
                schema,
                parameters: Vec::new(), // Parameters are in the definition
                return_type,
                definition,
            });
        }

        Ok(functions)
    }

    async fn get_triggers(&self, pool: &AnyPool) -> Result<Vec<Trigger>> {
        let query = r#"
            SELECT
                tgname as trigger_name,
                c.relname as table_name,
                CASE
                    WHEN tgtype & 2 = 2 THEN 'BEFORE'
                    WHEN tgtype & 64 = 64 THEN 'INSTEAD OF'
                    ELSE 'AFTER'
                END as timing,
                CASE
                    WHEN tgtype & 4 = 4 THEN 'INSERT'
                    WHEN tgtype & 8 = 8 THEN 'DELETE'
                    WHEN tgtype & 16 = 16 THEN 'UPDATE'
                    ELSE 'UNKNOWN'
                END as event,
                pg_get_triggerdef(t.oid) as definition
            FROM pg_trigger t
            JOIN pg_class c ON t.tgrelid = c.oid
            JOIN pg_namespace n ON c.relnamespace = n.oid
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
                AND NOT t.tgisinternal
            ORDER BY c.relname, tgname
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut triggers = Vec::new();
        for row in rows {
            let name: String = row.try_get("trigger_name")?;
            let table: String = row.try_get("table_name")?;
            let timing: String = row.try_get("timing")?;
            let event: String = row.try_get("event")?;
            let definition: String = row.try_get("definition")?;

            triggers.push(Trigger {
                name,
                table,
                timing,
                event,
                definition,
            });
        }

        Ok(triggers)
    }

    async fn get_sequences(&self, pool: &AnyPool) -> Result<Vec<Sequence>> {
        let query = r#"
            SELECT
                schemaname,
                sequencename,
                start_value,
                increment_by,
                min_value,
                max_value,
                cycle
            FROM pg_sequences
            WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY schemaname, sequencename
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut sequences = Vec::new();
        for row in rows {
            let schema: String = row.try_get("schemaname")?;
            let name: String = row.try_get("sequencename")?;
            let start_value: i64 = row.try_get("start_value")?;
            let increment: i64 = row.try_get("increment_by")?;
            let min_value: Option<i64> = row.try_get("min_value").ok();
            let max_value: Option<i64> = row.try_get("max_value").ok();
            let cycle: bool = row.try_get("cycle")?;

            sequences.push(Sequence {
                name,
                schema,
                start_value,
                increment,
                min_value,
                max_value,
                cycle,
            });
        }

        Ok(sequences)
    }
}

#[async_trait]
impl SchemaAnalyzer for PostgreSQLAnalyzer {
    async fn analyze_schema(&self, pool: &AnyPool, database_name: &str) -> Result<DatabaseSchema> {
        let tables = self.get_tables(pool).await?;
        let views = self.get_views(pool).await?;
        let procedures = self.get_procedures(pool).await?;
        let functions = self.get_functions(pool).await?;
        let triggers = self.get_triggers(pool).await?;
        let sequences = self.get_sequences(pool).await?;

        Ok(DatabaseSchema {
            connection_id: String::new(), // Will be set by caller
            database_name: database_name.to_string(),
            tables,
            views,
            procedures,
            functions,
            triggers,
            sequences,
            analyzed_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}


impl MySQLAnalyzer {
    pub fn new() -> Self {
        Self
    }

    async fn get_tables(&self, pool: &AnyPool, database: &str) -> Result<Vec<Table>> {
        let query = r#"
            SELECT 
                TABLE_SCHEMA,
                TABLE_NAME
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = ?
                AND TABLE_TYPE = 'BASE TABLE'
            ORDER BY TABLE_SCHEMA, TABLE_NAME
        "#;

        let rows = sqlx::query(query)
            .bind(database)
            .fetch_all(pool)
            .await
            .map_err(|e| SyncroDbError::SchemaAnalysis(format!("Failed to fetch tables: {}", e)))?;

        let mut tables = Vec::new();
        for row in rows {
            let schema: String = row.try_get("TABLE_SCHEMA")?;
            let name: String = row.try_get("TABLE_NAME")?;

            let columns = self.get_columns(pool, &schema, &name).await?;
            let primary_key = self.get_primary_key(pool, &schema, &name).await?;
            let foreign_keys = self.get_foreign_keys(pool, &schema, &name).await?;
            let unique_constraints = self.get_unique_constraints(pool, &schema, &name).await?;
            let check_constraints = self.get_check_constraints(pool, &schema, &name).await?;
            let indexes = self.get_indexes(pool, &schema, &name).await?;
            let row_count = self.get_row_count(pool, &schema, &name).await.ok();

            tables.push(Table {
                name,
                schema,
                columns,
                primary_key,
                foreign_keys,
                unique_constraints,
                check_constraints,
                indexes,
                row_count,
            });
        }

        Ok(tables)
    }

    async fn get_columns(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<Column>> {
        let query = r#"
            SELECT 
                COLUMN_NAME,
                DATA_TYPE,
                IS_NULLABLE,
                COLUMN_DEFAULT,
                ORDINAL_POSITION,
                COLUMN_COMMENT,
                EXTRA
            FROM information_schema.COLUMNS
            WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
            ORDER BY ORDINAL_POSITION
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut columns = Vec::new();
        for row in rows {
            let name: String = row.try_get("COLUMN_NAME")?;
            let data_type: String = row.try_get("DATA_TYPE")?;
            let is_nullable: String = row.try_get("IS_NULLABLE")?;
            let default_value: Option<String> = row.try_get("COLUMN_DEFAULT").ok();
            let ordinal_position: i32 = row.try_get("ORDINAL_POSITION")?;
            let comment: Option<String> = row.try_get("COLUMN_COMMENT").ok();
            let extra: String = row.try_get("EXTRA")?;

            let auto_increment = extra.to_lowercase().contains("auto_increment");

            columns.push(Column {
                name,
                data_type,
                nullable: is_nullable == "YES",
                default_value,
                auto_increment,
                comment,
                ordinal_position,
            });
        }

        Ok(columns)
    }

    async fn get_primary_key(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Option<PrimaryKey>> {
        let query = r#"
            SELECT
                CONSTRAINT_NAME,
                GROUP_CONCAT(COLUMN_NAME ORDER BY ORDINAL_POSITION) as columns
            FROM information_schema.KEY_COLUMN_USAGE
            WHERE CONSTRAINT_NAME = 'PRIMARY'
                AND TABLE_SCHEMA = ?
                AND TABLE_NAME = ?
            GROUP BY CONSTRAINT_NAME
        "#;

        let row = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_optional(pool)
            .await?;

        if let Some(row) = row {
            let name: String = row.try_get("CONSTRAINT_NAME")?;
            let columns_str: String = row.try_get("columns")?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();

            Ok(Some(PrimaryKey { name, columns }))
        } else {
            Ok(None)
        }
    }

    async fn get_foreign_keys(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<ForeignKey>> {
        let query = r#"
            SELECT
                kcu.CONSTRAINT_NAME,
                GROUP_CONCAT(kcu.COLUMN_NAME ORDER BY kcu.ORDINAL_POSITION) as columns,
                kcu.REFERENCED_TABLE_NAME,
                GROUP_CONCAT(kcu.REFERENCED_COLUMN_NAME ORDER BY kcu.ORDINAL_POSITION) as referenced_columns,
                rc.UPDATE_RULE,
                rc.DELETE_RULE
            FROM information_schema.KEY_COLUMN_USAGE kcu
            JOIN information_schema.REFERENTIAL_CONSTRAINTS rc
                ON kcu.CONSTRAINT_NAME = rc.CONSTRAINT_NAME
                AND kcu.TABLE_SCHEMA = rc.CONSTRAINT_SCHEMA
            WHERE kcu.TABLE_SCHEMA = ?
                AND kcu.TABLE_NAME = ?
                AND kcu.REFERENCED_TABLE_NAME IS NOT NULL
            GROUP BY kcu.CONSTRAINT_NAME, kcu.REFERENCED_TABLE_NAME, rc.UPDATE_RULE, rc.DELETE_RULE
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut foreign_keys = Vec::new();
        for row in rows {
            let name: String = row.try_get("CONSTRAINT_NAME")?;
            let columns_str: String = row.try_get("columns")?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();
            let referenced_table: String = row.try_get("REFERENCED_TABLE_NAME")?;
            let ref_columns_str: String = row.try_get("referenced_columns")?;
            let referenced_columns: Vec<String> = ref_columns_str.split(',').map(|s| s.to_string()).collect();
            let on_update: String = row.try_get("UPDATE_RULE")?;
            let on_delete: String = row.try_get("DELETE_RULE")?;

            foreign_keys.push(ForeignKey {
                name,
                columns,
                referenced_table,
                referenced_columns,
                on_update,
                on_delete,
            });
        }

        Ok(foreign_keys)
    }

    async fn get_unique_constraints(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<UniqueConstraint>> {
        let query = r#"
            SELECT
                tc.CONSTRAINT_NAME,
                GROUP_CONCAT(kcu.COLUMN_NAME ORDER BY kcu.ORDINAL_POSITION) as columns
            FROM information_schema.TABLE_CONSTRAINTS tc
            JOIN information_schema.KEY_COLUMN_USAGE kcu
                ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME
                AND tc.TABLE_SCHEMA = kcu.TABLE_SCHEMA
            WHERE tc.CONSTRAINT_TYPE = 'UNIQUE'
                AND tc.TABLE_SCHEMA = ?
                AND tc.TABLE_NAME = ?
            GROUP BY tc.CONSTRAINT_NAME
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut constraints = Vec::new();
        for row in rows {
            let name: String = row.try_get("CONSTRAINT_NAME")?;
            let columns_str: String = row.try_get("columns")?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();

            constraints.push(UniqueConstraint { name, columns });
        }

        Ok(constraints)
    }

    async fn get_check_constraints(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<CheckConstraint>> {
        let query = r#"
            SELECT
                CONSTRAINT_NAME,
                CHECK_CLAUSE
            FROM information_schema.CHECK_CONSTRAINTS
            WHERE CONSTRAINT_SCHEMA = ?
                AND TABLE_NAME = ?
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut constraints = Vec::new();
        for row in rows {
            let name: String = row.try_get("CONSTRAINT_NAME")?;
            let expression: String = row.try_get("CHECK_CLAUSE")?;

            constraints.push(CheckConstraint { name, expression });
        }

        Ok(constraints)
    }

    async fn get_indexes(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<Index>> {
        let query = r#"
            SELECT
                INDEX_NAME,
                GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX) as columns,
                NON_UNIQUE,
                INDEX_TYPE
            FROM information_schema.STATISTICS
            WHERE TABLE_SCHEMA = ?
                AND TABLE_NAME = ?
                AND INDEX_NAME != 'PRIMARY'
            GROUP BY INDEX_NAME, NON_UNIQUE, INDEX_TYPE
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut indexes = Vec::new();
        for row in rows {
            let name: String = row.try_get("INDEX_NAME")?;
            let columns_str: String = row.try_get("columns")?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();
            let non_unique: i32 = row.try_get("NON_UNIQUE")?;
            let index_type: String = row.try_get("INDEX_TYPE")?;

            indexes.push(Index {
                name,
                columns,
                unique: non_unique == 0,
                index_type,
                partial: false, // MySQL doesn't support partial indexes in the same way
                condition: None,
            });
        }

        Ok(indexes)
    }

    async fn get_row_count(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<i64> {
        let query = format!(
            "SELECT COUNT(*) as count FROM `{}`.`{}`",
            schema, table
        );

        let row = sqlx::query(&query)
            .fetch_one(pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count)
    }

    async fn get_views(&self, pool: &AnyPool, database: &str) -> Result<Vec<View>> {
        let query = r#"
            SELECT
                TABLE_SCHEMA,
                TABLE_NAME,
                VIEW_DEFINITION
            FROM information_schema.VIEWS
            WHERE TABLE_SCHEMA = ?
            ORDER BY TABLE_SCHEMA, TABLE_NAME
        "#;

        let rows = sqlx::query(query)
            .bind(database)
            .fetch_all(pool)
            .await?;

        let mut views = Vec::new();
        for row in rows {
            let schema: String = row.try_get("TABLE_SCHEMA")?;
            let name: String = row.try_get("TABLE_NAME")?;
            let definition: String = row.try_get("VIEW_DEFINITION")?;

            views.push(View {
                name,
                schema,
                definition,
            });
        }

        Ok(views)
    }

    async fn get_procedures(&self, pool: &AnyPool, database: &str) -> Result<Vec<StoredProcedure>> {
        let query = r#"
            SELECT
                ROUTINE_SCHEMA,
                ROUTINE_NAME,
                ROUTINE_DEFINITION
            FROM information_schema.ROUTINES
            WHERE ROUTINE_SCHEMA = ?
                AND ROUTINE_TYPE = 'PROCEDURE'
            ORDER BY ROUTINE_SCHEMA, ROUTINE_NAME
        "#;

        let rows = sqlx::query(query)
            .bind(database)
            .fetch_all(pool)
            .await?;

        let mut procedures = Vec::new();
        for row in rows {
            let schema: String = row.try_get("ROUTINE_SCHEMA")?;
            let name: String = row.try_get("ROUTINE_NAME")?;
            let definition: String = row.try_get("ROUTINE_DEFINITION").unwrap_or_default();

            procedures.push(StoredProcedure {
                name,
                schema,
                parameters: Vec::new(),
                definition,
            });
        }

        Ok(procedures)
    }

    async fn get_functions(&self, pool: &AnyPool, database: &str) -> Result<Vec<Function>> {
        let query = r#"
            SELECT
                ROUTINE_SCHEMA,
                ROUTINE_NAME,
                DTD_IDENTIFIER as return_type,
                ROUTINE_DEFINITION
            FROM information_schema.ROUTINES
            WHERE ROUTINE_SCHEMA = ?
                AND ROUTINE_TYPE = 'FUNCTION'
            ORDER BY ROUTINE_SCHEMA, ROUTINE_NAME
        "#;

        let rows = sqlx::query(query)
            .bind(database)
            .fetch_all(pool)
            .await?;

        let mut functions = Vec::new();
        for row in rows {
            let schema: String = row.try_get("ROUTINE_SCHEMA")?;
            let name: String = row.try_get("ROUTINE_NAME")?;
            let return_type: String = row.try_get("return_type")?;
            let definition: String = row.try_get("ROUTINE_DEFINITION").unwrap_or_default();

            functions.push(Function {
                name,
                schema,
                parameters: Vec::new(),
                return_type,
                definition,
            });
        }

        Ok(functions)
    }

    async fn get_triggers(&self, pool: &AnyPool, database: &str) -> Result<Vec<Trigger>> {
        let query = r#"
            SELECT
                TRIGGER_NAME,
                EVENT_OBJECT_TABLE as table_name,
                ACTION_TIMING as timing,
                EVENT_MANIPULATION as event,
                ACTION_STATEMENT as definition
            FROM information_schema.TRIGGERS
            WHERE TRIGGER_SCHEMA = ?
            ORDER BY EVENT_OBJECT_TABLE, TRIGGER_NAME
        "#;

        let rows = sqlx::query(query)
            .bind(database)
            .fetch_all(pool)
            .await?;

        let mut triggers = Vec::new();
        for row in rows {
            let name: String = row.try_get("TRIGGER_NAME")?;
            let table: String = row.try_get("table_name")?;
            let timing: String = row.try_get("timing")?;
            let event: String = row.try_get("event")?;
            let definition: String = row.try_get("definition")?;

            triggers.push(Trigger {
                name,
                table,
                timing,
                event,
                definition,
            });
        }

        Ok(triggers)
    }

    async fn get_sequences(&self, _pool: &AnyPool, _database: &str) -> Result<Vec<Sequence>> {
        // MySQL doesn't have sequences like PostgreSQL
        // Auto-increment is handled at the column level
        Ok(Vec::new())
    }
}

#[async_trait]
impl SchemaAnalyzer for MySQLAnalyzer {
    async fn analyze_schema(&self, pool: &AnyPool, database_name: &str) -> Result<DatabaseSchema> {
        let tables = self.get_tables(pool, database_name).await?;
        let views = self.get_views(pool, database_name).await?;
        let procedures = self.get_procedures(pool, database_name).await?;
        let functions = self.get_functions(pool, database_name).await?;
        let triggers = self.get_triggers(pool, database_name).await?;
        let sequences = self.get_sequences(pool, database_name).await?;

        Ok(DatabaseSchema {
            connection_id: String::new(),
            database_name: database_name.to_string(),
            tables,
            views,
            procedures,
            functions,
            triggers,
            sequences,
            analyzed_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}


impl SQLiteAnalyzer {
    pub fn new() -> Self {
        Self
    }

    async fn get_tables(&self, pool: &AnyPool) -> Result<Vec<Table>> {
        let query = r#"
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
                AND name NOT LIKE 'sqlite_%'
            ORDER BY name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await
            .map_err(|e| SyncroDbError::SchemaAnalysis(format!("Failed to fetch tables: {}", e)))?;

        let mut tables = Vec::new();
        for row in rows {
            let name: String = row.try_get("name")?;

            let columns = self.get_columns(pool, &name).await?;
            let primary_key = self.get_primary_key(pool, &name).await?;
            let foreign_keys = self.get_foreign_keys(pool, &name).await?;
            let unique_constraints = Vec::new(); // Handled in indexes
            let check_constraints = Vec::new(); // Would need to parse CREATE TABLE statement
            let indexes = self.get_indexes(pool, &name).await?;
            let row_count = self.get_row_count(pool, &name).await.ok();

            tables.push(Table {
                name,
                schema: "main".to_string(), // SQLite uses "main" as default schema
                columns,
                primary_key,
                foreign_keys,
                unique_constraints,
                check_constraints,
                indexes,
                row_count,
            });
        }

        Ok(tables)
    }

    async fn get_columns(&self, pool: &AnyPool, table: &str) -> Result<Vec<Column>> {
        let query = format!("PRAGMA table_info({})", table);

        let rows = sqlx::query(&query)
            .fetch_all(pool)
            .await?;

        let mut columns = Vec::new();
        for row in rows {
            let cid: i32 = row.try_get("cid")?;
            let name: String = row.try_get("name")?;
            let data_type: String = row.try_get("type")?;
            let not_null: i32 = row.try_get("notnull")?;
            let default_value: Option<String> = row.try_get("dflt_value").ok();
            let pk: i32 = row.try_get("pk")?;

            columns.push(Column {
                name,
                data_type,
                nullable: not_null == 0,
                default_value,
                auto_increment: pk > 0 && data_type.to_uppercase() == "INTEGER",
                comment: None,
                ordinal_position: cid + 1,
            });
        }

        Ok(columns)
    }

    async fn get_primary_key(&self, pool: &AnyPool, table: &str) -> Result<Option<PrimaryKey>> {
        let query = format!("PRAGMA table_info({})", table);

        let rows = sqlx::query(&query)
            .fetch_all(pool)
            .await?;

        let mut pk_columns = Vec::new();
        for row in rows {
            let pk: i32 = row.try_get("pk")?;
            if pk > 0 {
                let name: String = row.try_get("name")?;
                pk_columns.push((pk, name));
            }
        }

        if pk_columns.is_empty() {
            return Ok(None);
        }

        pk_columns.sort_by_key(|(order, _)| *order);
        let columns: Vec<String> = pk_columns.into_iter().map(|(_, name)| name).collect();

        Ok(Some(PrimaryKey {
            name: format!("{}_pk", table),
            columns,
        }))
    }

    async fn get_foreign_keys(&self, pool: &AnyPool, table: &str) -> Result<Vec<ForeignKey>> {
        let query = format!("PRAGMA foreign_key_list({})", table);

        let rows = sqlx::query(&query)
            .fetch_all(pool)
            .await?;

        let mut fk_map: HashMap<i32, (String, Vec<String>, String, Vec<String>, String, String)> = HashMap::new();

        for row in rows {
            let id: i32 = row.try_get("id")?;
            let seq: i32 = row.try_get("seq")?;
            let table_ref: String = row.try_get("table")?;
            let from_col: String = row.try_get("from")?;
            let to_col: String = row.try_get("to")?;
            let on_update: String = row.try_get("on_update")?;
            let on_delete: String = row.try_get("on_delete")?;

            let entry = fk_map.entry(id).or_insert_with(|| {
                (
                    format!("{}_fk_{}", table, id),
                    Vec::new(),
                    table_ref.clone(),
                    Vec::new(),
                    on_update.clone(),
                    on_delete.clone(),
                )
            });

            entry.1.push(from_col);
            entry.3.push(to_col);
        }

        let foreign_keys = fk_map
            .into_iter()
            .map(|(_, (name, columns, referenced_table, referenced_columns, on_update, on_delete))| {
                ForeignKey {
                    name,
                    columns,
                    referenced_table,
                    referenced_columns,
                    on_update,
                    on_delete,
                }
            })
            .collect();

        Ok(foreign_keys)
    }

    async fn get_indexes(&self, pool: &AnyPool, table: &str) -> Result<Vec<Index>> {
        let query = format!("PRAGMA index_list({})", table);

        let rows = sqlx::query(&query)
            .fetch_all(pool)
            .await?;

        let mut indexes = Vec::new();
        for row in rows {
            let name: String = row.try_get("name")?;
            let unique: i32 = row.try_get("unique")?;
            let partial: i32 = row.try_get("partial")?;

            // Get index columns
            let col_query = format!("PRAGMA index_info({})", name);
            let col_rows = sqlx::query(&col_query)
                .fetch_all(pool)
                .await?;

            let mut columns = Vec::new();
            for col_row in col_rows {
                if let Ok(col_name) = col_row.try_get::<String, _>("name") {
                    columns.push(col_name);
                }
            }

            indexes.push(Index {
                name,
                columns,
                unique: unique == 1,
                index_type: "BTREE".to_string(), // SQLite primarily uses B-tree
                partial: partial == 1,
                condition: None, // Would need to parse from sqlite_master
            });
        }

        Ok(indexes)
    }

    async fn get_row_count(&self, pool: &AnyPool, table: &str) -> Result<i64> {
        let query = format!("SELECT COUNT(*) as count FROM {}", table);

        let row = sqlx::query(&query)
            .fetch_one(pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count)
    }

    async fn get_views(&self, pool: &AnyPool) -> Result<Vec<View>> {
        let query = r#"
            SELECT name, sql
            FROM sqlite_master
            WHERE type = 'view'
            ORDER BY name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut views = Vec::new();
        for row in rows {
            let name: String = row.try_get("name")?;
            let definition: String = row.try_get("sql")?;

            views.push(View {
                name,
                schema: "main".to_string(),
                definition,
            });
        }

        Ok(views)
    }

    async fn get_triggers(&self, pool: &AnyPool) -> Result<Vec<Trigger>> {
        let query = r#"
            SELECT name, tbl_name, sql
            FROM sqlite_master
            WHERE type = 'trigger'
            ORDER BY tbl_name, name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut triggers = Vec::new();
        for row in rows {
            let name: String = row.try_get("name")?;
            let table: String = row.try_get("tbl_name")?;
            let definition: String = row.try_get("sql")?;

            // Parse timing and event from SQL (simplified)
            let timing = if definition.to_uppercase().contains("BEFORE") {
                "BEFORE"
            } else if definition.to_uppercase().contains("AFTER") {
                "AFTER"
            } else {
                "INSTEAD OF"
            };

            let event = if definition.to_uppercase().contains("INSERT") {
                "INSERT"
            } else if definition.to_uppercase().contains("UPDATE") {
                "UPDATE"
            } else if definition.to_uppercase().contains("DELETE") {
                "DELETE"
            } else {
                "UNKNOWN"
            };

            triggers.push(Trigger {
                name,
                table,
                timing: timing.to_string(),
                event: event.to_string(),
                definition,
            });
        }

        Ok(triggers)
    }
}

#[async_trait]
impl SchemaAnalyzer for SQLiteAnalyzer {
    async fn analyze_schema(&self, pool: &AnyPool, database_name: &str) -> Result<DatabaseSchema> {
        let tables = self.get_tables(pool).await?;
        let views = self.get_views(pool).await?;
        let triggers = self.get_triggers(pool).await?;

        // SQLite doesn't support stored procedures or functions
        let procedures = Vec::new();
        let functions = Vec::new();
        let sequences = Vec::new();

        Ok(DatabaseSchema {
            connection_id: String::new(),
            database_name: database_name.to_string(),
            tables,
            views,
            procedures,
            functions,
            triggers,
            sequences,
            analyzed_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}


impl SQLServerAnalyzer {
    pub fn new() -> Self {
        Self
    }

    async fn get_tables(&self, pool: &AnyPool) -> Result<Vec<Table>> {
        let query = r#"
            SELECT 
                s.name as schema_name,
                t.name as table_name
            FROM sys.tables t
            JOIN sys.schemas s ON t.schema_id = s.schema_id
            WHERE t.is_ms_shipped = 0
            ORDER BY s.name, t.name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await
            .map_err(|e| SyncroDbError::SchemaAnalysis(format!("Failed to fetch tables: {}", e)))?;

        let mut tables = Vec::new();
        for row in rows {
            let schema: String = row.try_get("schema_name")?;
            let name: String = row.try_get("table_name")?;

            let columns = self.get_columns(pool, &schema, &name).await?;
            let primary_key = self.get_primary_key(pool, &schema, &name).await?;
            let foreign_keys = self.get_foreign_keys(pool, &schema, &name).await?;
            let unique_constraints = self.get_unique_constraints(pool, &schema, &name).await?;
            let check_constraints = self.get_check_constraints(pool, &schema, &name).await?;
            let indexes = self.get_indexes(pool, &schema, &name).await?;
            let row_count = self.get_row_count(pool, &schema, &name).await.ok();

            tables.push(Table {
                name,
                schema,
                columns,
                primary_key,
                foreign_keys,
                unique_constraints,
                check_constraints,
                indexes,
                row_count,
            });
        }

        Ok(tables)
    }

    async fn get_columns(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<Column>> {
        let query = r#"
            SELECT 
                c.name as column_name,
                t.name as data_type,
                c.is_nullable,
                dc.definition as default_value,
                c.column_id,
                c.is_identity,
                ep.value as comment
            FROM sys.columns c
            JOIN sys.tables tbl ON c.object_id = tbl.object_id
            JOIN sys.schemas s ON tbl.schema_id = s.schema_id
            JOIN sys.types t ON c.user_type_id = t.user_type_id
            LEFT JOIN sys.default_constraints dc ON c.default_object_id = dc.object_id
            LEFT JOIN sys.extended_properties ep ON ep.major_id = c.object_id 
                AND ep.minor_id = c.column_id 
                AND ep.name = 'MS_Description'
            WHERE s.name = @p1 AND tbl.name = @p2
            ORDER BY c.column_id
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut columns = Vec::new();
        for row in rows {
            let name: String = row.try_get("column_name")?;
            let data_type: String = row.try_get("data_type")?;
            let is_nullable: bool = row.try_get("is_nullable")?;
            let default_value: Option<String> = row.try_get("default_value").ok();
            let ordinal_position: i32 = row.try_get("column_id")?;
            let is_identity: bool = row.try_get("is_identity")?;
            let comment: Option<String> = row.try_get("comment").ok();

            columns.push(Column {
                name,
                data_type,
                nullable: is_nullable,
                default_value,
                auto_increment: is_identity,
                comment,
                ordinal_position,
            });
        }

        Ok(columns)
    }

    async fn get_primary_key(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Option<PrimaryKey>> {
        let query = r#"
            SELECT 
                kc.name as constraint_name,
                STRING_AGG(c.name, ',') WITHIN GROUP (ORDER BY ic.key_ordinal) as columns
            FROM sys.key_constraints kc
            JOIN sys.tables t ON kc.parent_object_id = t.object_id
            JOIN sys.schemas s ON t.schema_id = s.schema_id
            JOIN sys.index_columns ic ON kc.parent_object_id = ic.object_id 
                AND kc.unique_index_id = ic.index_id
            JOIN sys.columns c ON ic.object_id = c.object_id 
                AND ic.column_id = c.column_id
            WHERE kc.type = 'PK'
                AND s.name = @p1
                AND t.name = @p2
            GROUP BY kc.name
        "#;

        let row = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_optional(pool)
            .await?;

        if let Some(row) = row {
            let name: String = row.try_get("constraint_name")?;
            let columns_str: String = row.try_get("columns")?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();

            Ok(Some(PrimaryKey { name, columns }))
        } else {
            Ok(None)
        }
    }

    async fn get_foreign_keys(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<ForeignKey>> {
        let query = r#"
            SELECT 
                fk.name as constraint_name,
                STRING_AGG(c.name, ',') WITHIN GROUP (ORDER BY fkc.constraint_column_id) as columns,
                rt.name as referenced_table,
                STRING_AGG(rc.name, ',') WITHIN GROUP (ORDER BY fkc.constraint_column_id) as referenced_columns,
                fk.update_referential_action_desc as on_update,
                fk.delete_referential_action_desc as on_delete
            FROM sys.foreign_keys fk
            JOIN sys.tables t ON fk.parent_object_id = t.object_id
            JOIN sys.schemas s ON t.schema_id = s.schema_id
            JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id
            JOIN sys.columns c ON fkc.parent_object_id = c.object_id 
                AND fkc.parent_column_id = c.column_id
            JOIN sys.tables rt ON fk.referenced_object_id = rt.object_id
            JOIN sys.columns rc ON fkc.referenced_object_id = rc.object_id 
                AND fkc.referenced_column_id = rc.column_id
            WHERE s.name = @p1 AND t.name = @p2
            GROUP BY fk.name, rt.name, fk.update_referential_action_desc, fk.delete_referential_action_desc
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut foreign_keys = Vec::new();
        for row in rows {
            let name: String = row.try_get("constraint_name")?;
            let columns_str: String = row.try_get("columns")?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();
            let referenced_table: String = row.try_get("referenced_table")?;
            let ref_columns_str: String = row.try_get("referenced_columns")?;
            let referenced_columns: Vec<String> = ref_columns_str.split(',').map(|s| s.to_string()).collect();
            let on_update: String = row.try_get("on_update")?;
            let on_delete: String = row.try_get("on_delete")?;

            foreign_keys.push(ForeignKey {
                name,
                columns,
                referenced_table,
                referenced_columns,
                on_update,
                on_delete,
            });
        }

        Ok(foreign_keys)
    }

    async fn get_unique_constraints(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<UniqueConstraint>> {
        let query = r#"
            SELECT 
                kc.name as constraint_name,
                STRING_AGG(c.name, ',') WITHIN GROUP (ORDER BY ic.key_ordinal) as columns
            FROM sys.key_constraints kc
            JOIN sys.tables t ON kc.parent_object_id = t.object_id
            JOIN sys.schemas s ON t.schema_id = s.schema_id
            JOIN sys.index_columns ic ON kc.parent_object_id = ic.object_id 
                AND kc.unique_index_id = ic.index_id
            JOIN sys.columns c ON ic.object_id = c.object_id 
                AND ic.column_id = c.column_id
            WHERE kc.type = 'UQ'
                AND s.name = @p1
                AND t.name = @p2
            GROUP BY kc.name
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut constraints = Vec::new();
        for row in rows {
            let name: String = row.try_get("constraint_name")?;
            let columns_str: String = row.try_get("columns")?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();

            constraints.push(UniqueConstraint { name, columns });
        }

        Ok(constraints)
    }

    async fn get_check_constraints(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<CheckConstraint>> {
        let query = r#"
            SELECT 
                cc.name as constraint_name,
                cc.definition
            FROM sys.check_constraints cc
            JOIN sys.tables t ON cc.parent_object_id = t.object_id
            JOIN sys.schemas s ON t.schema_id = s.schema_id
            WHERE s.name = @p1 AND t.name = @p2
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut constraints = Vec::new();
        for row in rows {
            let name: String = row.try_get("constraint_name")?;
            let expression: String = row.try_get("definition")?;

            constraints.push(CheckConstraint { name, expression });
        }

        Ok(constraints)
    }

    async fn get_indexes(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<Vec<Index>> {
        let query = r#"
            SELECT 
                i.name as index_name,
                STRING_AGG(c.name, ',') WITHIN GROUP (ORDER BY ic.key_ordinal) as columns,
                i.is_unique,
                i.type_desc as index_type,
                i.has_filter,
                i.filter_definition
            FROM sys.indexes i
            JOIN sys.tables t ON i.object_id = t.object_id
            JOIN sys.schemas s ON t.schema_id = s.schema_id
            JOIN sys.index_columns ic ON i.object_id = ic.object_id 
                AND i.index_id = ic.index_id
            JOIN sys.columns c ON ic.object_id = c.object_id 
                AND ic.column_id = c.column_id
            WHERE s.name = @p1 
                AND t.name = @p2
                AND i.is_primary_key = 0
                AND i.type > 0
            GROUP BY i.name, i.is_unique, i.type_desc, i.has_filter, i.filter_definition
        "#;

        let rows = sqlx::query(query)
            .bind(schema)
            .bind(table)
            .fetch_all(pool)
            .await?;

        let mut indexes = Vec::new();
        for row in rows {
            let name: String = row.try_get("index_name")?;
            let columns_str: String = row.try_get("columns")?;
            let columns: Vec<String> = columns_str.split(',').map(|s| s.to_string()).collect();
            let unique: bool = row.try_get("is_unique")?;
            let index_type: String = row.try_get("index_type")?;
            let has_filter: bool = row.try_get("has_filter")?;
            let condition: Option<String> = row.try_get("filter_definition").ok();

            indexes.push(Index {
                name,
                columns,
                unique,
                index_type,
                partial: has_filter,
                condition,
            });
        }

        Ok(indexes)
    }

    async fn get_row_count(&self, pool: &AnyPool, schema: &str, table: &str) -> Result<i64> {
        let query = format!(
            "SELECT COUNT(*) as count FROM [{}].[{}]",
            schema, table
        );

        let row = sqlx::query(&query)
            .fetch_one(pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count)
    }

    async fn get_views(&self, pool: &AnyPool) -> Result<Vec<View>> {
        let query = r#"
            SELECT 
                s.name as schema_name,
                v.name as view_name,
                m.definition
            FROM sys.views v
            JOIN sys.schemas s ON v.schema_id = s.schema_id
            JOIN sys.sql_modules m ON v.object_id = m.object_id
            WHERE v.is_ms_shipped = 0
            ORDER BY s.name, v.name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut views = Vec::new();
        for row in rows {
            let schema: String = row.try_get("schema_name")?;
            let name: String = row.try_get("view_name")?;
            let definition: String = row.try_get("definition")?;

            views.push(View {
                name,
                schema,
                definition,
            });
        }

        Ok(views)
    }

    async fn get_procedures(&self, pool: &AnyPool) -> Result<Vec<StoredProcedure>> {
        let query = r#"
            SELECT 
                s.name as schema_name,
                p.name as procedure_name,
                m.definition
            FROM sys.procedures p
            JOIN sys.schemas s ON p.schema_id = s.schema_id
            JOIN sys.sql_modules m ON p.object_id = m.object_id
            WHERE p.is_ms_shipped = 0
            ORDER BY s.name, p.name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut procedures = Vec::new();
        for row in rows {
            let schema: String = row.try_get("schema_name")?;
            let name: String = row.try_get("procedure_name")?;
            let definition: String = row.try_get("definition")?;

            procedures.push(StoredProcedure {
                name,
                schema,
                parameters: Vec::new(),
                definition,
            });
        }

        Ok(procedures)
    }

    async fn get_functions(&self, pool: &AnyPool) -> Result<Vec<Function>> {
        let query = r#"
            SELECT 
                s.name as schema_name,
                o.name as function_name,
                t.name as return_type,
                m.definition
            FROM sys.objects o
            JOIN sys.schemas s ON o.schema_id = s.schema_id
            JOIN sys.sql_modules m ON o.object_id = m.object_id
            LEFT JOIN sys.parameters p ON o.object_id = p.object_id AND p.parameter_id = 0
            LEFT JOIN sys.types t ON p.user_type_id = t.user_type_id
            WHERE o.type IN ('FN', 'IF', 'TF')
                AND o.is_ms_shipped = 0
            ORDER BY s.name, o.name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut functions = Vec::new();
        for row in rows {
            let schema: String = row.try_get("schema_name")?;
            let name: String = row.try_get("function_name")?;
            let return_type: String = row.try_get("return_type").unwrap_or_else(|_| "TABLE".to_string());
            let definition: String = row.try_get("definition")?;

            functions.push(Function {
                name,
                schema,
                parameters: Vec::new(),
                return_type,
                definition,
            });
        }

        Ok(functions)
    }

    async fn get_triggers(&self, pool: &AnyPool) -> Result<Vec<Trigger>> {
        let query = r#"
            SELECT 
                tr.name as trigger_name,
                t.name as table_name,
                CASE 
                    WHEN tr.is_instead_of_trigger = 1 THEN 'INSTEAD OF'
                    ELSE 'AFTER'
                END as timing,
                CASE 
                    WHEN OBJECTPROPERTY(tr.object_id, 'ExecIsInsertTrigger') = 1 THEN 'INSERT'
                    WHEN OBJECTPROPERTY(tr.object_id, 'ExecIsUpdateTrigger') = 1 THEN 'UPDATE'
                    WHEN OBJECTPROPERTY(tr.object_id, 'ExecIsDeleteTrigger') = 1 THEN 'DELETE'
                    ELSE 'UNKNOWN'
                END as event,
                m.definition
            FROM sys.triggers tr
            JOIN sys.tables t ON tr.parent_id = t.object_id
            JOIN sys.sql_modules m ON tr.object_id = m.object_id
            WHERE tr.is_ms_shipped = 0
            ORDER BY t.name, tr.name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut triggers = Vec::new();
        for row in rows {
            let name: String = row.try_get("trigger_name")?;
            let table: String = row.try_get("table_name")?;
            let timing: String = row.try_get("timing")?;
            let event: String = row.try_get("event")?;
            let definition: String = row.try_get("definition")?;

            triggers.push(Trigger {
                name,
                table,
                timing,
                event,
                definition,
            });
        }

        Ok(triggers)
    }

    async fn get_sequences(&self, pool: &AnyPool) -> Result<Vec<Sequence>> {
        let query = r#"
            SELECT 
                s.name as schema_name,
                seq.name as sequence_name,
                seq.start_value,
                seq.increment,
                seq.minimum_value,
                seq.maximum_value,
                seq.is_cycling
            FROM sys.sequences seq
            JOIN sys.schemas s ON seq.schema_id = s.schema_id
            ORDER BY s.name, seq.name
        "#;

        let rows = sqlx::query(query)
            .fetch_all(pool)
            .await?;

        let mut sequences = Vec::new();
        for row in rows {
            let schema: String = row.try_get("schema_name")?;
            let name: String = row.try_get("sequence_name")?;
            let start_value: i64 = row.try_get("start_value")?;
            let increment: i64 = row.try_get("increment")?;
            let min_value: Option<i64> = row.try_get("minimum_value").ok();
            let max_value: Option<i64> = row.try_get("maximum_value").ok();
            let cycle: bool = row.try_get("is_cycling")?;

            sequences.push(Sequence {
                name,
                schema,
                start_value,
                increment,
                min_value,
                max_value,
                cycle,
            });
        }

        Ok(sequences)
    }
}

#[async_trait]
impl SchemaAnalyzer for SQLServerAnalyzer {
    async fn analyze_schema(&self, pool: &AnyPool, database_name: &str) -> Result<DatabaseSchema> {
        let tables = self.get_tables(pool).await?;
        let views = self.get_views(pool).await?;
        let procedures = self.get_procedures(pool).await?;
        let functions = self.get_functions(pool).await?;
        let triggers = self.get_triggers(pool).await?;
        let sequences = self.get_sequences(pool).await?;

        Ok(DatabaseSchema {
            connection_id: String::new(),
            database_name: database_name.to_string(),
            tables,
            views,
            procedures,
            functions,
            triggers,
            sequences,
            analyzed_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}
