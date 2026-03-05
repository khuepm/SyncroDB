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
