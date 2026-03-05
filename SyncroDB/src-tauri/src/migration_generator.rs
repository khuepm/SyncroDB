use crate::error::{Result, SyncroDbError};
use crate::models::*;
use std::collections::{HashMap, HashSet};

pub struct MigrationGenerator {
    db_type: DatabaseType,
}

impl MigrationGenerator {
    pub fn new(db_type: DatabaseType) -> Self {
        Self { db_type }
    }

    /// Generate migration script from schema differences
    pub fn generate_script(
        &self,
        differences: &[SchemaDifference],
        selected_operation_ids: &[String],
    ) -> Result<String> {
        // Filter to only selected operations
        let selected_diffs: Vec<&SchemaDifference> = differences
            .iter()
            .filter(|d| selected_operation_ids.contains(&d.id))
            .collect();

        if selected_diffs.is_empty() {
            return Ok(String::new());
        }

        // Sort operations by dependencies
        let sorted_diffs = self.sort_operations_by_dependencies(&selected_diffs)?;

        // Generate SQL for each operation
        let mut script = String::new();
        
        // Add transaction begin
        script.push_str(&self.generate_transaction_begin());
        script.push_str("\n\n");

        for (idx, diff) in sorted_diffs.iter().enumerate() {
            // Add savepoint before each operation
            script.push_str(&format!("SAVEPOINT sp_operation_{};\n", idx + 1));
            
            // Add safety comment for destructive operations
            if diff.destructive {
                script.push_str(&self.generate_safety_comment(diff));
            }
            
            // Generate SQL for the operation
            let sql = self.generate_sql_for_difference(diff)?;
            script.push_str(&sql);
            script.push_str("\n\n");
        }

        // Add transaction commit
        script.push_str(&self.generate_transaction_commit());

        Ok(script)
    }

    /// Sort operations by dependencies (tables before foreign keys, etc.)
    fn sort_operations_by_dependencies(
        &self,
        diffs: &[&SchemaDifference],
    ) -> Result<Vec<&SchemaDifference>> {
        let mut sorted = Vec::new();
        let mut remaining: Vec<&SchemaDifference> = diffs.to_vec();

        // Priority order for additions:
        // 1. Tables
        // 2. Columns
        // 3. Primary keys
        // 4. Unique constraints
        // 5. Check constraints
        // 6. Indexes
        // 7. Foreign keys (depend on referenced tables)
        // 8. Views, Procedures, Functions, Triggers, Sequences

        // For deletions, reverse the order
        let priority_order_additions = vec![
            SchemaObjectType::Table,
            SchemaObjectType::Column,
            SchemaObjectType::PrimaryKey,
            SchemaObjectType::UniqueConstraint,
            SchemaObjectType::CheckConstraint,
            SchemaObjectType::Index,
            SchemaObjectType::ForeignKey,
            SchemaObjectType::Sequence,
            SchemaObjectType::View,
            SchemaObjectType::Function,
            SchemaObjectType::Procedure,
            SchemaObjectType::Trigger,
        ];

        let priority_order_deletions = vec![
            SchemaObjectType::Trigger,
            SchemaObjectType::View,
            SchemaObjectType::ForeignKey,
            SchemaObjectType::Index,
            SchemaObjectType::CheckConstraint,
            SchemaObjectType::UniqueConstraint,
            SchemaObjectType::PrimaryKey,
            SchemaObjectType::Column,
            SchemaObjectType::Table,
            SchemaObjectType::Procedure,
            SchemaObjectType::Function,
            SchemaObjectType::Sequence,
        ];

        // Process additions first
        for obj_type in &priority_order_additions {
            let mut i = 0;
            while i < remaining.len() {
                if remaining[i].object_type == *obj_type 
                    && remaining[i].change_type == ChangeType::Addition {
                    sorted.push(remaining.remove(i));
                } else {
                    i += 1;
                }
            }
        }

        // Then modifications
        for obj_type in &priority_order_additions {
            let mut i = 0;
            while i < remaining.len() {
                if remaining[i].object_type == *obj_type 
                    && remaining[i].change_type == ChangeType::Modification {
                    sorted.push(remaining.remove(i));
                } else {
                    i += 1;
                }
            }
        }

        // Finally deletions (in reverse order)
        for obj_type in &priority_order_deletions {
            let mut i = 0;
            while i < remaining.len() {
                if remaining[i].object_type == *obj_type 
                    && remaining[i].change_type == ChangeType::Deletion {
                    sorted.push(remaining.remove(i));
                } else {
                    i += 1;
                }
            }
        }

        // Add any remaining items
        sorted.extend(remaining);

        Ok(sorted)
    }

    /// Generate SQL for a specific difference
    fn generate_sql_for_difference(&self, diff: &SchemaDifference) -> Result<String> {
        match self.db_type {
            DatabaseType::PostgreSQL => self.generate_postgresql_sql(diff),
            DatabaseType::MySQL => self.generate_mysql_sql(diff),
            DatabaseType::SQLite => self.generate_sqlite_sql(diff),
            DatabaseType::SQLServer => self.generate_sqlserver_sql(diff),
        }
    }

    /// Generate transaction begin statement
    fn generate_transaction_begin(&self) -> String {
        match self.db_type {
            DatabaseType::PostgreSQL | DatabaseType::SQLite => "BEGIN;".to_string(),
            DatabaseType::MySQL => "START TRANSACTION;".to_string(),
            DatabaseType::SQLServer => "BEGIN TRANSACTION;".to_string(),
        }
    }

    /// Generate transaction commit statement
    fn generate_transaction_commit(&self) -> String {
        "COMMIT;".to_string()
    }

    /// Generate safety comment for destructive operations
    fn generate_safety_comment(&self, diff: &SchemaDifference) -> String {
        format!(
            "-- WARNING: DESTRUCTIVE OPERATION\n\
             -- This operation will {} {}\n\
             -- Object: {}\n\
             -- Review carefully before executing\n",
            match diff.change_type {
                ChangeType::Deletion => "drop",
                ChangeType::Modification => "modify",
                _ => "affect",
            },
            format!("{:?}", diff.object_type).to_lowercase(),
            diff.object_name
        )
    }

    // PostgreSQL SQL Generation
    fn generate_postgresql_sql(&self, diff: &SchemaDifference) -> Result<String> {
        match (&diff.object_type, &diff.change_type) {
            (SchemaObjectType::Table, ChangeType::Addition) => {
                self.generate_postgresql_create_table(diff)
            }
            (SchemaObjectType::Table, ChangeType::Deletion) => {
                self.generate_postgresql_drop_table(diff)
            }
            (SchemaObjectType::Column, ChangeType::Addition) => {
                self.generate_postgresql_add_column(diff)
            }
            (SchemaObjectType::Column, ChangeType::Deletion) => {
                self.generate_postgresql_drop_column(diff)
            }
            (SchemaObjectType::Column, ChangeType::Modification) => {
                self.generate_postgresql_alter_column(diff)
            }
            (SchemaObjectType::PrimaryKey, ChangeType::Addition) => {
                self.generate_postgresql_add_primary_key(diff)
            }
            (SchemaObjectType::PrimaryKey, ChangeType::Deletion) => {
                self.generate_postgresql_drop_constraint(diff)
            }
            (SchemaObjectType::ForeignKey, ChangeType::Addition) => {
                self.generate_postgresql_add_foreign_key(diff)
            }
            (SchemaObjectType::ForeignKey, ChangeType::Deletion) => {
                self.generate_postgresql_drop_constraint(diff)
            }
            (SchemaObjectType::UniqueConstraint, ChangeType::Addition) => {
                self.generate_postgresql_add_unique_constraint(diff)
            }
            (SchemaObjectType::UniqueConstraint, ChangeType::Deletion) => {
                self.generate_postgresql_drop_constraint(diff)
            }
            (SchemaObjectType::CheckConstraint, ChangeType::Addition) => {
                self.generate_postgresql_add_check_constraint(diff)
            }
            (SchemaObjectType::CheckConstraint, ChangeType::Deletion) => {
                self.generate_postgresql_drop_constraint(diff)
            }
            (SchemaObjectType::Index, ChangeType::Addition) => {
                self.generate_postgresql_create_index(diff)
            }
            (SchemaObjectType::Index, ChangeType::Deletion) => {
                self.generate_postgresql_drop_index(diff)
            }
            (SchemaObjectType::View, ChangeType::Addition) => {
                self.generate_postgresql_create_view(diff)
            }
            (SchemaObjectType::View, ChangeType::Deletion) => {
                self.generate_postgresql_drop_view(diff)
            }
            (SchemaObjectType::View, ChangeType::Modification) => {
                self.generate_postgresql_replace_view(diff)
            }
            (SchemaObjectType::Procedure, ChangeType::Addition) => {
                self.generate_postgresql_create_procedure(diff)
            }
            (SchemaObjectType::Procedure, ChangeType::Deletion) => {
                self.generate_postgresql_drop_procedure(diff)
            }
            (SchemaObjectType::Function, ChangeType::Addition) => {
                self.generate_postgresql_create_function(diff)
            }
            (SchemaObjectType::Function, ChangeType::Deletion) => {
                self.generate_postgresql_drop_function(diff)
            }
            (SchemaObjectType::Trigger, ChangeType::Addition) => {
                self.generate_postgresql_create_trigger(diff)
            }
            (SchemaObjectType::Trigger, ChangeType::Deletion) => {
                self.generate_postgresql_drop_trigger(diff)
            }
            (SchemaObjectType::Sequence, ChangeType::Addition) => {
                self.generate_postgresql_create_sequence(diff)
            }
            (SchemaObjectType::Sequence, ChangeType::Deletion) => {
                self.generate_postgresql_drop_sequence(diff)
            }
            _ => Err(SyncroDbError::Migration(format!(
                "Unsupported operation: {:?} {:?}",
                diff.object_type, diff.change_type
            ))),
        }
    }

    // PostgreSQL Template Methods
    fn generate_postgresql_create_table(&self, diff: &SchemaDifference) -> Result<String> {
        let table: Table = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for table creation".to_string())
            })?
        )?;

        let mut sql = format!("CREATE TABLE {}.{} (\n", table.schema, table.name);

        // Add columns
        let column_defs: Vec<String> = table.columns.iter().map(|col| {
            format!(
                "    {} {} {}{}",
                col.name,
                col.data_type,
                if col.nullable { "NULL" } else { "NOT NULL" },
                col.default_value.as_ref().map(|d| format!(" DEFAULT {}", d)).unwrap_or_default()
            )
        }).collect();

        sql.push_str(&column_defs.join(",\n"));
        sql.push_str("\n);");

        Ok(sql)
    }

    fn generate_postgresql_drop_table(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }
        Ok(format!("DROP TABLE {}.{};", parts[0], parts[1]))
    }

    fn generate_postgresql_add_column(&self, diff: &SchemaDifference) -> Result<String> {
        let column: Column = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for column".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(SyncroDbError::Migration("Invalid column name format".to_string()));
        }
        let table_name = parts[1];
        let schema_name = parts[2];

        Ok(format!(
            "ALTER TABLE {}.{}\nADD COLUMN {} {} {}{}",
            schema_name,
            table_name,
            column.name,
            column.data_type,
            if column.nullable { "NULL" } else { "NOT NULL" },
            column.default_value.as_ref().map(|d| format!(" DEFAULT {}", d)).unwrap_or_default()
        ))
    }

    fn generate_postgresql_drop_column(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.rsplitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(SyncroDbError::Migration("Invalid column name format".to_string()));
        }
        let column_name = parts[0];
        let table_name = parts[1];
        let schema_name = parts[2];

        Ok(format!(
            "ALTER TABLE {}.{}\nDROP COLUMN {};",
            schema_name, table_name, column_name
        ))
    }

    fn generate_postgresql_alter_column(&self, diff: &SchemaDifference) -> Result<String> {
        let source_col: Column = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for column".to_string())
            })?
        )?;
        let target_col: Column = serde_json::from_value(
            diff.target_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing target value for column".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(SyncroDbError::Migration("Invalid column name format".to_string()));
        }
        let column_name = parts[0];
        let table_name = parts[1];
        let schema_name = parts[2];

        let mut statements = Vec::new();

        // Type change
        if source_col.data_type != target_col.data_type {
            statements.push(format!(
                "ALTER TABLE {}.{}\nALTER COLUMN {} TYPE {} USING {}::{};",
                schema_name, table_name, column_name, source_col.data_type, column_name, source_col.data_type
            ));
        }

        // Nullability change
        if source_col.nullable != target_col.nullable {
            statements.push(format!(
                "ALTER TABLE {}.{}\nALTER COLUMN {} {} NOT NULL;",
                schema_name,
                table_name,
                column_name,
                if source_col.nullable { "DROP" } else { "SET" }
            ));
        }

        // Default value change
        if source_col.default_value != target_col.default_value {
            if let Some(default) = &source_col.default_value {
                statements.push(format!(
                    "ALTER TABLE {}.{}\nALTER COLUMN {} SET DEFAULT {};",
                    schema_name, table_name, column_name, default
                ));
            } else {
                statements.push(format!(
                    "ALTER TABLE {}.{}\nALTER COLUMN {} DROP DEFAULT;",
                    schema_name, table_name, column_name
                ));
            }
        }

        Ok(statements.join("\n"))
    }

    fn generate_postgresql_add_primary_key(&self, diff: &SchemaDifference) -> Result<String> {
        let pk: PrimaryKey = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for primary key".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid primary key name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE {}.{}\nADD CONSTRAINT {} PRIMARY KEY ({});",
            table_parts[0],
            table_parts[1],
            pk.name,
            pk.columns.join(", ")
        ))
    }

    fn generate_postgresql_drop_constraint(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid constraint name format".to_string()));
        }
        let constraint_name = parts[0];
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE {}.{}\nDROP CONSTRAINT {};",
            table_parts[0], table_parts[1], constraint_name
        ))
    }

    fn generate_postgresql_add_foreign_key(&self, diff: &SchemaDifference) -> Result<String> {
        let fk: ForeignKey = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for foreign key".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid foreign key name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE {}.{}\nADD CONSTRAINT {} FOREIGN KEY ({})\n    REFERENCES {} ({})\n    ON DELETE {}\n    ON UPDATE {};",
            table_parts[0],
            table_parts[1],
            fk.name,
            fk.columns.join(", "),
            fk.referenced_table,
            fk.referenced_columns.join(", "),
            fk.on_delete,
            fk.on_update
        ))
    }

    fn generate_postgresql_add_unique_constraint(&self, diff: &SchemaDifference) -> Result<String> {
        let uc: UniqueConstraint = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for unique constraint".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid unique constraint name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE {}.{}\nADD CONSTRAINT {} UNIQUE ({});",
            table_parts[0],
            table_parts[1],
            uc.name,
            uc.columns.join(", ")
        ))
    }

    fn generate_postgresql_add_check_constraint(&self, diff: &SchemaDifference) -> Result<String> {
        let cc: CheckConstraint = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for check constraint".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid check constraint name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE {}.{}\nADD CONSTRAINT {} CHECK ({});",
            table_parts[0],
            table_parts[1],
            cc.name,
            cc.expression
        ))
    }

    fn generate_postgresql_create_index(&self, diff: &SchemaDifference) -> Result<String> {
        let idx: Index = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for index".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid index name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        let unique_clause = if idx.unique { "UNIQUE " } else { "" };
        let using_clause = if idx.index_type != "BTREE" {
            format!(" USING {}", idx.index_type)
        } else {
            String::new()
        };
        let where_clause = if let Some(condition) = &idx.condition {
            format!(" WHERE {}", condition)
        } else {
            String::new()
        };

        Ok(format!(
            "CREATE {}INDEX {} ON {}.{}{} ({}){};",
            unique_clause,
            idx.name,
            table_parts[0],
            table_parts[1],
            using_clause,
            idx.columns.join(", "),
            where_clause
        ))
    }

    fn generate_postgresql_drop_index(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid index name format".to_string()));
        }
        let index_name = parts[0];
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!("DROP INDEX {}.{};", table_parts[0], index_name))
    }

    fn generate_postgresql_create_view(&self, diff: &SchemaDifference) -> Result<String> {
        let view: View = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for view".to_string())
            })?
        )?;

        Ok(format!(
            "CREATE VIEW {}.{} AS\n{};",
            view.schema, view.name, view.definition
        ))
    }

    fn generate_postgresql_drop_view(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid view name format".to_string()));
        }
        Ok(format!("DROP VIEW {}.{};", parts[0], parts[1]))
    }

    fn generate_postgresql_replace_view(&self, diff: &SchemaDifference) -> Result<String> {
        let view: View = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for view".to_string())
            })?
        )?;

        Ok(format!(
            "CREATE OR REPLACE VIEW {}.{} AS\n{};",
            view.schema, view.name, view.definition
        ))
    }

    fn generate_postgresql_create_procedure(&self, diff: &SchemaDifference) -> Result<String> {
        let proc: StoredProcedure = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for procedure".to_string())
            })?
        )?;

        Ok(format!(
            "CREATE PROCEDURE {}.{}\n{};",
            proc.schema, proc.name, proc.definition
        ))
    }

    fn generate_postgresql_drop_procedure(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid procedure name format".to_string()));
        }
        Ok(format!("DROP PROCEDURE {}.{};", parts[0], parts[1]))
    }

    fn generate_postgresql_create_function(&self, diff: &SchemaDifference) -> Result<String> {
        let func: Function = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for function".to_string())
            })?
        )?;

        Ok(format!(
            "CREATE FUNCTION {}.{}\n{};",
            func.schema, func.name, func.definition
        ))
    }

    fn generate_postgresql_drop_function(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid function name format".to_string()));
        }
        Ok(format!("DROP FUNCTION {}.{};", parts[0], parts[1]))
    }

    fn generate_postgresql_create_trigger(&self, diff: &SchemaDifference) -> Result<String> {
        let trigger: Trigger = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for trigger".to_string())
            })?
        )?;

        Ok(format!(
            "CREATE TRIGGER {}\n{};",
            trigger.name, trigger.definition
        ))
    }

    fn generate_postgresql_drop_trigger(&self, diff: &SchemaDifference) -> Result<String> {
        let trigger: Trigger = serde_json::from_value(
            diff.target_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing target value for trigger".to_string())
            })?
        )?;

        Ok(format!("DROP TRIGGER {} ON {};", trigger.name, trigger.table))
    }

    fn generate_postgresql_create_sequence(&self, diff: &SchemaDifference) -> Result<String> {
        let seq: Sequence = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for sequence".to_string())
            })?
        )?;

        let mut sql = format!(
            "CREATE SEQUENCE {}.{}\n    START WITH {}\n    INCREMENT BY {}",
            seq.schema, seq.name, seq.start_value, seq.increment
        );

        if let Some(min) = seq.min_value {
            sql.push_str(&format!("\n    MINVALUE {}", min));
        }
        if let Some(max) = seq.max_value {
            sql.push_str(&format!("\n    MAXVALUE {}", max));
        }
        if seq.cycle {
            sql.push_str("\n    CYCLE");
        }
        sql.push(';');

        Ok(sql)
    }

    fn generate_postgresql_drop_sequence(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid sequence name format".to_string()));
        }
        Ok(format!("DROP SEQUENCE {}.{};", parts[0], parts[1]))
    }

    // Placeholder methods for other database types (to be implemented in subsequent tasks)
    fn generate_mysql_sql(&self, diff: &SchemaDifference) -> Result<String> {
        match (&diff.object_type, &diff.change_type) {
            (SchemaObjectType::Table, ChangeType::Addition) => {
                self.generate_mysql_create_table(diff)
            }
            (SchemaObjectType::Table, ChangeType::Deletion) => {
                self.generate_mysql_drop_table(diff)
            }
            (SchemaObjectType::Column, ChangeType::Addition) => {
                self.generate_mysql_add_column(diff)
            }
            (SchemaObjectType::Column, ChangeType::Deletion) => {
                self.generate_mysql_drop_column(diff)
            }
            (SchemaObjectType::Column, ChangeType::Modification) => {
                self.generate_mysql_alter_column(diff)
            }
            (SchemaObjectType::PrimaryKey, ChangeType::Addition) => {
                self.generate_mysql_add_primary_key(diff)
            }
            (SchemaObjectType::PrimaryKey, ChangeType::Deletion) => {
                self.generate_mysql_drop_constraint(diff, "PRIMARY KEY")
            }
            (SchemaObjectType::ForeignKey, ChangeType::Addition) => {
                self.generate_mysql_add_foreign_key(diff)
            }
            (SchemaObjectType::ForeignKey, ChangeType::Deletion) => {
                self.generate_mysql_drop_foreign_key(diff)
            }
            (SchemaObjectType::UniqueConstraint, ChangeType::Addition) => {
                self.generate_mysql_add_unique_constraint(diff)
            }
            (SchemaObjectType::UniqueConstraint, ChangeType::Deletion) => {
                self.generate_mysql_drop_index(diff)
            }
            (SchemaObjectType::CheckConstraint, ChangeType::Addition) => {
                self.generate_mysql_add_check_constraint(diff)
            }
            (SchemaObjectType::CheckConstraint, ChangeType::Deletion) => {
                self.generate_mysql_drop_check_constraint(diff)
            }
            (SchemaObjectType::Index, ChangeType::Addition) => {
                self.generate_mysql_create_index(diff)
            }
            (SchemaObjectType::Index, ChangeType::Deletion) => {
                self.generate_mysql_drop_index(diff)
            }
            (SchemaObjectType::View, ChangeType::Addition) => {
                self.generate_mysql_create_view(diff)
            }
            (SchemaObjectType::View, ChangeType::Deletion) => {
                self.generate_mysql_drop_view(diff)
            }
            (SchemaObjectType::View, ChangeType::Modification) => {
                self.generate_mysql_replace_view(diff)
            }
            (SchemaObjectType::Procedure, ChangeType::Addition) => {
                self.generate_mysql_create_procedure(diff)
            }
            (SchemaObjectType::Procedure, ChangeType::Deletion) => {
                self.generate_mysql_drop_procedure(diff)
            }
            (SchemaObjectType::Function, ChangeType::Addition) => {
                self.generate_mysql_create_function(diff)
            }
            (SchemaObjectType::Function, ChangeType::Deletion) => {
                self.generate_mysql_drop_function(diff)
            }
            (SchemaObjectType::Trigger, ChangeType::Addition) => {
                self.generate_mysql_create_trigger(diff)
            }
            (SchemaObjectType::Trigger, ChangeType::Deletion) => {
                self.generate_mysql_drop_trigger(diff)
            }
            _ => Err(SyncroDbError::Migration(format!(
                "Unsupported MySQL operation: {:?} {:?}",
                diff.object_type, diff.change_type
            ))),
        }
    }

    // MySQL Template Methods
    fn generate_mysql_create_table(&self, diff: &SchemaDifference) -> Result<String> {
        let table: Table = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for table creation".to_string())
            })?
        )?;

        let mut sql = format!("CREATE TABLE `{}`.`{}` (\n", table.schema, table.name);

        // Add columns
        let column_defs: Vec<String> = table.columns.iter().map(|col| {
            let auto_inc = if col.auto_increment { " AUTO_INCREMENT" } else { "" };
            format!(
                "    `{}` {} {}{}{}",
                col.name,
                col.data_type,
                if col.nullable { "NULL" } else { "NOT NULL" },
                col.default_value.as_ref().map(|d| format!(" DEFAULT {}", d)).unwrap_or_default(),
                auto_inc
            )
        }).collect();

        sql.push_str(&column_defs.join(",\n"));
        sql.push_str("\n) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;");

        Ok(sql)
    }

    fn generate_mysql_drop_table(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }
        Ok(format!("DROP TABLE `{}`.`{}`;", parts[0], parts[1]))
    }

    fn generate_mysql_add_column(&self, diff: &SchemaDifference) -> Result<String> {
        let column: Column = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for column".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(SyncroDbError::Migration("Invalid column name format".to_string()));
        }
        let table_name = parts[1];
        let schema_name = parts[2];

        let auto_inc = if column.auto_increment { " AUTO_INCREMENT" } else { "" };

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nADD COLUMN `{}` {} {}{}{}",
            schema_name,
            table_name,
            column.name,
            column.data_type,
            if column.nullable { "NULL" } else { "NOT NULL" },
            column.default_value.as_ref().map(|d| format!(" DEFAULT {}", d)).unwrap_or_default(),
            auto_inc
        ))
    }

    fn generate_mysql_drop_column(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.rsplitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(SyncroDbError::Migration("Invalid column name format".to_string()));
        }
        let column_name = parts[0];
        let table_name = parts[1];
        let schema_name = parts[2];

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nDROP COLUMN `{}`;",
            schema_name, table_name, column_name
        ))
    }

    fn generate_mysql_alter_column(&self, diff: &SchemaDifference) -> Result<String> {
        let source_col: Column = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for column".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(SyncroDbError::Migration("Invalid column name format".to_string()));
        }
        let column_name = parts[0];
        let table_name = parts[1];
        let schema_name = parts[2];

        let auto_inc = if source_col.auto_increment { " AUTO_INCREMENT" } else { "" };

        // MySQL uses MODIFY COLUMN for all column changes
        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nMODIFY COLUMN `{}` {} {}{}{}",
            schema_name,
            table_name,
            column_name,
            source_col.data_type,
            if source_col.nullable { "NULL" } else { "NOT NULL" },
            source_col.default_value.as_ref().map(|d| format!(" DEFAULT {}", d)).unwrap_or_default(),
            auto_inc
        ))
    }

    fn generate_mysql_add_primary_key(&self, diff: &SchemaDifference) -> Result<String> {
        let pk: PrimaryKey = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for primary key".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid primary key name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        let columns: Vec<String> = pk.columns.iter().map(|c| format!("`{}`", c)).collect();

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nADD PRIMARY KEY ({});",
            table_parts[0],
            table_parts[1],
            columns.join(", ")
        ))
    }

    fn generate_mysql_drop_constraint(&self, diff: &SchemaDifference, constraint_type: &str) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid constraint name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nDROP {};",
            table_parts[0], table_parts[1], constraint_type
        ))
    }

    fn generate_mysql_add_foreign_key(&self, diff: &SchemaDifference) -> Result<String> {
        let fk: ForeignKey = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for foreign key".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid foreign key name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        let columns: Vec<String> = fk.columns.iter().map(|c| format!("`{}`", c)).collect();
        let ref_columns: Vec<String> = fk.referenced_columns.iter().map(|c| format!("`{}`", c)).collect();

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nADD CONSTRAINT `{}` FOREIGN KEY ({})\n    REFERENCES `{}` ({})\n    ON DELETE {}\n    ON UPDATE {};",
            table_parts[0],
            table_parts[1],
            fk.name,
            columns.join(", "),
            fk.referenced_table,
            ref_columns.join(", "),
            fk.on_delete,
            fk.on_update
        ))
    }

    fn generate_mysql_drop_foreign_key(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid foreign key name format".to_string()));
        }
        let constraint_name = parts[0];
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nDROP FOREIGN KEY `{}`;",
            table_parts[0], table_parts[1], constraint_name
        ))
    }

    fn generate_mysql_add_unique_constraint(&self, diff: &SchemaDifference) -> Result<String> {
        let uc: UniqueConstraint = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for unique constraint".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid unique constraint name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        let columns: Vec<String> = uc.columns.iter().map(|c| format!("`{}`", c)).collect();

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nADD UNIQUE INDEX `{}` ({});",
            table_parts[0],
            table_parts[1],
            uc.name,
            columns.join(", ")
        ))
    }

    fn generate_mysql_add_check_constraint(&self, diff: &SchemaDifference) -> Result<String> {
        let cc: CheckConstraint = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for check constraint".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid check constraint name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nADD CONSTRAINT `{}` CHECK ({});",
            table_parts[0],
            table_parts[1],
            cc.name,
            cc.expression
        ))
    }

    fn generate_mysql_drop_check_constraint(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid check constraint name format".to_string()));
        }
        let constraint_name = parts[0];
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!(
            "ALTER TABLE `{}`.`{}`\nDROP CHECK `{}`;",
            table_parts[0], table_parts[1], constraint_name
        ))
    }

    fn generate_mysql_create_index(&self, diff: &SchemaDifference) -> Result<String> {
        let idx: Index = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for index".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid index name format".to_string()));
        }
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        let unique_clause = if idx.unique { "UNIQUE " } else { "" };
        let using_clause = if idx.index_type != "BTREE" {
            format!(" USING {}", idx.index_type)
        } else {
            String::new()
        };
        let columns: Vec<String> = idx.columns.iter().map(|c| format!("`{}`", c)).collect();

        Ok(format!(
            "CREATE {}INDEX `{}` ON `{}`.`{}`{} ({});",
            unique_clause,
            idx.name,
            table_parts[0],
            table_parts[1],
            using_clause,
            columns.join(", ")
        ))
    }

    fn generate_mysql_drop_index(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid index name format".to_string()));
        }
        let index_name = parts[0];
        let table_parts: Vec<&str> = parts[1].split('.').collect();
        if table_parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid table name format".to_string()));
        }

        Ok(format!("DROP INDEX `{}` ON `{}`.`{}`;", index_name, table_parts[0], table_parts[1]))
    }

    fn generate_mysql_create_view(&self, diff: &SchemaDifference) -> Result<String> {
        let view: View = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for view".to_string())
            })?
        )?;

        Ok(format!(
            "CREATE VIEW `{}`.`{}` AS\n{};",
            view.schema, view.name, view.definition
        ))
    }

    fn generate_mysql_drop_view(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid view name format".to_string()));
        }
        Ok(format!("DROP VIEW `{}`.`{}`;", parts[0], parts[1]))
    }

    fn generate_mysql_replace_view(&self, diff: &SchemaDifference) -> Result<String> {
        let view: View = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for view".to_string())
            })?
        )?;

        Ok(format!(
            "CREATE OR REPLACE VIEW `{}`.`{}` AS\n{};",
            view.schema, view.name, view.definition
        ))
    }

    fn generate_mysql_create_procedure(&self, diff: &SchemaDifference) -> Result<String> {
        let proc: StoredProcedure = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for procedure".to_string())
            })?
        )?;

        Ok(format!(
            "DELIMITER //\nCREATE PROCEDURE `{}`.`{}`\n{}\n//\nDELIMITER ;",
            proc.schema, proc.name, proc.definition
        ))
    }

    fn generate_mysql_drop_procedure(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid procedure name format".to_string()));
        }
        Ok(format!("DROP PROCEDURE `{}`.`{}`;", parts[0], parts[1]))
    }

    fn generate_mysql_create_function(&self, diff: &SchemaDifference) -> Result<String> {
        let func: Function = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for function".to_string())
            })?
        )?;

        Ok(format!(
            "DELIMITER //\nCREATE FUNCTION `{}`.`{}`\n{}\n//\nDELIMITER ;",
            func.schema, func.name, func.definition
        ))
    }

    fn generate_mysql_drop_function(&self, diff: &SchemaDifference) -> Result<String> {
        let parts: Vec<&str> = diff.object_name.split('.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid function name format".to_string()));
        }
        Ok(format!("DROP FUNCTION `{}`.`{}`;", parts[0], parts[1]))
    }

    fn generate_mysql_create_trigger(&self, diff: &SchemaDifference) -> Result<String> {
        let trigger: Trigger = serde_json::from_value(
            diff.source_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing source value for trigger".to_string())
            })?
        )?;

        Ok(format!(
            "DELIMITER //\nCREATE TRIGGER `{}`\n{}\n//\nDELIMITER ;",
            trigger.name, trigger.definition
        ))
    }

    fn generate_mysql_drop_trigger(&self, diff: &SchemaDifference) -> Result<String> {
        let trigger: Trigger = serde_json::from_value(
            diff.target_value.clone().ok_or_else(|| {
                SyncroDbError::Migration("Missing target value for trigger".to_string())
            })?
        )?;

        let parts: Vec<&str> = diff.object_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(SyncroDbError::Migration("Invalid trigger name format".to_string()));
        }
        let trigger_name = parts[0];

        Ok(format!("DROP TRIGGER `{}`;", trigger_name))
    }

    fn generate_sqlite_sql(&self, _diff: &SchemaDifference) -> Result<String> {
        Err(SyncroDbError::Migration("SQLite SQL generation not yet implemented".to_string()))
    }

    fn generate_sqlserver_sql(&self, _diff: &SchemaDifference) -> Result<String> {
        Err(SyncroDbError::Migration("SQL Server SQL generation not yet implemented".to_string()))
    }
}

impl Default for MigrationGenerator {
    fn default() -> Self {
        Self::new(DatabaseType::PostgreSQL)
    }
}
