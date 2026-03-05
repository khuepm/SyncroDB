// Feature: syncrodb-schema-sync, Property 6: SQL Generation Correctness
// **Validates: Requirements 4.1, 4.2, 4.3, 8.7**

use crate::migration_generator::MigrationGenerator;
use crate::models::*;
use proptest::prelude::*;
use uuid::Uuid;

// Arbitrary generators for testing

fn arbitrary_database_type() -> impl Strategy<Value = DatabaseType> {
    prop_oneof![
        Just(DatabaseType::PostgreSQL),
        Just(DatabaseType::MySQL),
        Just(DatabaseType::SQLite),
        Just(DatabaseType::SQLServer),
    ]
}

fn arbitrary_column() -> impl Strategy<Value = Column> {
    (
        "[a-z][a-z0-9_]{0,20}",
        "(VARCHAR\\(50\\)|INTEGER|BIGINT|TEXT|BOOLEAN)",
        any::<bool>(),
        prop::option::of("'default_value'"),
        any::<bool>(),
        prop::option::of("[a-z ]{5,20}"),
        1i32..100i32,
    )
        .prop_map(
            |(name, data_type, nullable, default_value, auto_increment, comment, ordinal_position)| {
                Column {
                    name,
                    data_type,
                    nullable,
                    default_value,
                    auto_increment,
                    comment,
                    ordinal_position,
                }
            },
        )
}

fn arbitrary_table() -> impl Strategy<Value = Table> {
    (
        "[a-z][a-z0-9_]{0,20}",
        "(public|dbo|main)",
        prop::collection::vec(arbitrary_column(), 1..5),
    )
        .prop_map(|(name, schema, columns)| Table {
            name,
            schema,
            columns,
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
            row_count: None,
        })
}

fn arbitrary_view() -> impl Strategy<Value = View> {
    (
        "[a-z][a-z0-9_]{0,20}",
        "(public|dbo|main)",
        "SELECT \\* FROM test_table",
    )
        .prop_map(|(name, schema, definition)| View {
            name,
            schema,
            definition: definition.to_string(),
        })
}

fn arbitrary_index() -> impl Strategy<Value = Index> {
    (
        "[a-z][a-z0-9_]{0,20}",
        prop::collection::vec("[a-z][a-z0-9_]{0,20}", 1..3),
        any::<bool>(),
        "(BTREE|HASH)",
        any::<bool>(),
        prop::option::of("column_name > 0"),
    )
        .prop_map(|(name, columns, unique, index_type, partial, condition)| Index {
            name,
            columns,
            unique,
            index_type,
            partial,
            condition,
        })
}

fn arbitrary_schema_difference_table_addition() -> impl Strategy<Value = SchemaDifference> {
    arbitrary_table().prop_map(|table| SchemaDifference {
        id: Uuid::new_v4().to_string(),
        object_type: SchemaObjectType::Table,
        object_name: format!("{}.{}", table.schema, table.name),
        change_type: ChangeType::Addition,
        source_value: Some(serde_json::to_value(&table).unwrap()),
        target_value: None,
        details: format!("Table {} needs to be added", table.name),
        destructive: false,
    })
}

fn arbitrary_schema_difference_column_addition() -> impl Strategy<Value = SchemaDifference> {
    (arbitrary_column(), "[a-z][a-z0-9_]{0,20}", "(public|dbo|main)").prop_map(
        |(column, table_name, schema)| SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Column,
            object_name: format!("{}.{}.{}", schema, table_name, column.name),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&column).unwrap()),
            target_value: None,
            details: format!("Column {} needs to be added", column.name),
            destructive: false,
        },
    )
}

fn arbitrary_schema_difference_view_addition() -> impl Strategy<Value = SchemaDifference> {
    arbitrary_view().prop_map(|view| SchemaDifference {
        id: Uuid::new_v4().to_string(),
        object_type: SchemaObjectType::View,
        object_name: format!("{}.{}", view.schema, view.name),
        change_type: ChangeType::Addition,
        source_value: Some(serde_json::to_value(&view).unwrap()),
        target_value: None,
        details: format!("View {} needs to be added", view.name),
        destructive: false,
    })
}

fn arbitrary_schema_difference_index_addition() -> impl Strategy<Value = SchemaDifference> {
    (arbitrary_index(), "[a-z][a-z0-9_]{0,20}", "(public|dbo|main)").prop_map(
        |(index, table_name, schema)| SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Index,
            object_name: format!("{}.{}.{}", schema, table_name, index.name),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&index).unwrap()),
            target_value: None,
            details: format!("Index {} needs to be added", index.name),
            destructive: false,
        },
    )
}

fn arbitrary_schema_difference() -> impl Strategy<Value = SchemaDifference> {
    prop_oneof![
        arbitrary_schema_difference_table_addition(),
        arbitrary_schema_difference_column_addition(),
        arbitrary_schema_difference_view_addition(),
        arbitrary_schema_difference_index_addition(),
    ]
}

// Property Tests

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Property 6: SQL Generation Correctness
    /// For any set of approved migration operations and target database type,
    /// the migration generator should create syntactically valid SQL scripts
    /// in the correct dialect for that database type, with transaction boundaries
    /// (BEGIN/COMMIT/ROLLBACK) and appropriate SQL statements for all supported
    /// schema object types.
    #[test]
    fn test_sql_generation_has_transaction_boundaries(
        db_type in arbitrary_database_type(),
        differences in prop::collection::vec(arbitrary_schema_difference(), 1..10)
    ) {
        let generator = MigrationGenerator::new(db_type.clone());
        let operation_ids: Vec<String> = differences.iter().map(|d| d.id.clone()).collect();
        
        let result = generator.generate_script(&differences, &operation_ids);
        
        // Should succeed for supported operations
        if let Ok(script) = result {
            // Verify transaction boundaries exist
            match db_type {
                DatabaseType::PostgreSQL | DatabaseType::SQLite => {
                    assert!(script.contains("BEGIN;"), "Script should contain BEGIN transaction");
                    assert!(script.contains("COMMIT;"), "Script should contain COMMIT");
                }
                DatabaseType::MySQL => {
                    assert!(script.contains("START TRANSACTION;"), "Script should contain START TRANSACTION");
                    assert!(script.contains("COMMIT;"), "Script should contain COMMIT");
                }
                DatabaseType::SQLServer => {
                    assert!(script.contains("BEGIN TRANSACTION;"), "Script should contain BEGIN TRANSACTION");
                    assert!(script.contains("COMMIT;"), "Script should contain COMMIT");
                }
            }
            
            // Verify savepoints are present
            assert!(script.contains("SAVEPOINT sp_operation_"), "Script should contain savepoints");
        }
    }

    #[test]
    fn test_sql_generation_uses_correct_dialect(
        db_type in arbitrary_database_type(),
        differences in prop::collection::vec(arbitrary_schema_difference(), 1..5)
    ) {
        let generator = MigrationGenerator::new(db_type.clone());
        let operation_ids: Vec<String> = differences.iter().map(|d| d.id.clone()).collect();
        
        let result = generator.generate_script(&differences, &operation_ids);
        
        if let Ok(script) = result {
            // Verify database-specific syntax
            match db_type {
                DatabaseType::PostgreSQL => {
                    // PostgreSQL uses unquoted or double-quoted identifiers
                    // Should not contain MySQL backticks or SQL Server brackets
                    assert!(!script.contains("`"), "PostgreSQL should not use backticks");
                    assert!(!script.contains("["), "PostgreSQL should not use square brackets");
                }
                DatabaseType::MySQL => {
                    // MySQL uses backticks for identifiers
                    if script.contains("CREATE TABLE") || script.contains("ALTER TABLE") {
                        assert!(script.contains("`"), "MySQL should use backticks for identifiers");
                    }
                }
                DatabaseType::SQLite => {
                    // SQLite uses double quotes for identifiers
                    if script.contains("CREATE TABLE") || script.contains("ALTER TABLE") {
                        assert!(script.contains("\""), "SQLite should use double quotes for identifiers");
                    }
                }
                DatabaseType::SQLServer => {
                    // SQL Server uses square brackets for identifiers
                    if script.contains("CREATE TABLE") || script.contains("ALTER TABLE") {
                        assert!(script.contains("["), "SQL Server should use square brackets for identifiers");
                    }
                }
            }
        }
    }

    #[test]
    fn test_sql_generation_handles_all_operation_types(
        db_type in arbitrary_database_type(),
        diff in arbitrary_schema_difference()
    ) {
        let generator = MigrationGenerator::new(db_type);
        let operation_ids = vec![diff.id.clone()];
        
        let result = generator.generate_script(&[diff.clone()], &operation_ids);
        
        // Should either succeed or provide clear error for unsupported operations
        match result {
            Ok(script) => {
                // Script should not be empty
                assert!(!script.trim().is_empty(), "Generated script should not be empty");
                
                // Should contain appropriate DDL keywords based on operation type
                match diff.object_type {
                    SchemaObjectType::Table => {
                        let upper_script = script.to_uppercase();
                        assert!(upper_script.contains("CREATE TABLE") || upper_script.contains("DROP TABLE") || upper_script.contains("ALTER TABLE"));
                    }
                    SchemaObjectType::Column => {
                        let upper_script = script.to_uppercase();
                        // Column operations should contain ALTER TABLE and either ADD/DROP/ALTER/MODIFY
                        assert!(
                            upper_script.contains("ALTER TABLE") && (
                                upper_script.contains("ADD") || 
                                upper_script.contains("DROP") || 
                                upper_script.contains("ALTER") || 
                                upper_script.contains("MODIFY")
                            ),
                            "Column operation should contain ALTER TABLE and ADD/DROP/ALTER/MODIFY"
                        );
                    }
                    SchemaObjectType::View => {
                        let upper_script = script.to_uppercase();
                        assert!(upper_script.contains("CREATE VIEW") || upper_script.contains("DROP VIEW") || upper_script.contains("ALTER VIEW"));
                    }
                    SchemaObjectType::Index => {
                        let upper_script = script.to_uppercase();
                        assert!((upper_script.contains("CREATE") && upper_script.contains("INDEX")) || upper_script.contains("DROP INDEX"));
                    }
                    _ => {}
                }
            }
            Err(e) => {
                // Error should be descriptive
                let error_msg = e.to_string();
                assert!(!error_msg.is_empty(), "Error message should not be empty");
                assert!(
                    error_msg.contains("not") || error_msg.contains("unsupported") || error_msg.contains("SQLite"),
                    "Error should explain why operation is not supported"
                );
            }
        }
    }

    #[test]
    fn test_sql_generation_empty_selection_returns_empty_script(
        db_type in arbitrary_database_type(),
        differences in prop::collection::vec(arbitrary_schema_difference(), 1..5)
    ) {
        let generator = MigrationGenerator::new(db_type);
        
        // Pass empty selection
        let result = generator.generate_script(&differences, &[]);
        
        assert!(result.is_ok(), "Should succeed with empty selection");
        assert_eq!(result.unwrap(), "", "Should return empty script for empty selection");
    }

    #[test]
    fn test_sql_generation_maintains_operation_order(
        db_type in arbitrary_database_type(),
        differences in prop::collection::vec(arbitrary_schema_difference(), 2..10)
    ) {
        let generator = MigrationGenerator::new(db_type);
        let operation_ids: Vec<String> = differences.iter().map(|d| d.id.clone()).collect();
        
        let result = generator.generate_script(&differences, &operation_ids);
        
        if let Ok(script) = result {
            // Count savepoints - should match number of operations
            let savepoint_count = script.matches("SAVEPOINT sp_operation_").count();
            assert_eq!(
                savepoint_count,
                differences.len(),
                "Should have one savepoint per operation"
            );
            
            // Verify savepoints are numbered sequentially
            for i in 1..=differences.len() {
                assert!(
                    script.contains(&format!("SAVEPOINT sp_operation_{}", i)),
                    "Should have savepoint for operation {}", i
                );
            }
        }
    }
}

// Unit tests for specific scenarios

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_postgresql_create_table_syntax() {
        let generator = MigrationGenerator::new(DatabaseType::PostgreSQL);
        
        let table = Table {
            name: "users".to_string(),
            schema: "public".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    auto_increment: false,
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "name".to_string(),
                    data_type: "VARCHAR(100)".to_string(),
                    nullable: true,
                    default_value: None,
                    auto_increment: false,
                    comment: None,
                    ordinal_position: 2,
                },
            ],
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
            row_count: None,
        };

        let diff = SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Table,
            object_name: "public.users".to_string(),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&table).unwrap()),
            target_value: None,
            details: "Table users needs to be added".to_string(),
            destructive: false,
        };

        let script = generator.generate_script(&[diff.clone()], &[diff.id]).unwrap();

        assert!(script.contains("BEGIN;"));
        assert!(script.contains("CREATE TABLE public.users"));
        assert!(script.contains("id INTEGER NOT NULL"));
        assert!(script.contains("name VARCHAR(100) NULL"));
        assert!(script.contains("COMMIT;"));
    }

    #[test]
    fn test_mysql_uses_backticks() {
        let generator = MigrationGenerator::new(DatabaseType::MySQL);
        
        let table = Table {
            name: "products".to_string(),
            schema: "main".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "INT".to_string(),
                    nullable: false,
                    default_value: None,
                    auto_increment: true,
                    comment: None,
                    ordinal_position: 1,
                },
            ],
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
            row_count: None,
        };

        let diff = SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Table,
            object_name: "main.products".to_string(),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&table).unwrap()),
            target_value: None,
            details: "Table products needs to be added".to_string(),
            destructive: false,
        };

        let script = generator.generate_script(&[diff.clone()], &[diff.id]).unwrap();

        assert!(script.contains("`main`.`products`"));
        assert!(script.contains("`id` INT NOT NULL AUTO_INCREMENT"));
        assert!(script.contains("ENGINE=InnoDB"));
    }

    #[test]
    fn test_destructive_operations_have_warnings() {
        let generator = MigrationGenerator::new(DatabaseType::PostgreSQL);
        
        let diff = SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Table,
            object_name: "public.old_table".to_string(),
            change_type: ChangeType::Deletion,
            source_value: None,
            target_value: None,
            details: "Table old_table needs to be dropped".to_string(),
            destructive: true,
        };

        let script = generator.generate_script(&[diff.clone()], &[diff.id]).unwrap();

        assert!(script.contains("WARNING: DESTRUCTIVE OPERATION"));
        assert!(script.contains("DROP TABLE public.old_table"));
    }

    #[test]
    fn test_sqlite_create_table_with_constraints() {
        let generator = MigrationGenerator::new(DatabaseType::SQLite);
        
        let table = Table {
            name: "orders".to_string(),
            schema: "main".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    auto_increment: true,
                    comment: None,
                    ordinal_position: 1,
                },
                Column {
                    name: "customer_id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    auto_increment: false,
                    comment: None,
                    ordinal_position: 2,
                },
            ],
            primary_key: Some(PrimaryKey {
                name: "pk_orders".to_string(),
                columns: vec!["id".to_string()],
            }),
            foreign_keys: vec![ForeignKey {
                name: "fk_customer".to_string(),
                columns: vec!["customer_id".to_string()],
                referenced_table: "customers".to_string(),
                referenced_columns: vec!["id".to_string()],
                on_delete: "CASCADE".to_string(),
                on_update: "NO ACTION".to_string(),
            }],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
            row_count: None,
        };

        let diff = SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Table,
            object_name: "main.orders".to_string(),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&table).unwrap()),
            target_value: None,
            details: "Table orders needs to be added".to_string(),
            destructive: false,
        };

        let script = generator.generate_script(&[diff.clone()], &[diff.id]).unwrap();

        assert!(script.contains("CREATE TABLE \"orders\""));
        assert!(script.contains("PRIMARY KEY"));
        assert!(script.contains("FOREIGN KEY"));
        assert!(script.contains("REFERENCES \"customers\""));
    }

    #[test]
    fn test_sqlserver_uses_square_brackets() {
        let generator = MigrationGenerator::new(DatabaseType::SQLServer);
        
        let table = Table {
            name: "employees".to_string(),
            schema: "dbo".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "INT".to_string(),
                    nullable: false,
                    default_value: None,
                    auto_increment: true,
                    comment: None,
                    ordinal_position: 1,
                },
            ],
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
            row_count: None,
        };

        let diff = SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Table,
            object_name: "dbo.employees".to_string(),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&table).unwrap()),
            target_value: None,
            details: "Table employees needs to be added".to_string(),
            destructive: false,
        };

        let script = generator.generate_script(&[diff.clone()], &[diff.id]).unwrap();

        assert!(script.contains("[dbo].[employees]"));
        assert!(script.contains("[id] INT NOT NULL IDENTITY(1,1)"));
    }

    #[test]
    fn test_operation_ordering_tables_before_foreign_keys() {
        let generator = MigrationGenerator::new(DatabaseType::PostgreSQL);
        
        // Create a foreign key difference (should come after table)
        let fk_diff = SchemaDifference {
            id: "fk-1".to_string(),
            object_type: SchemaObjectType::ForeignKey,
            object_name: "public.orders.fk_customer".to_string(),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&ForeignKey {
                name: "fk_customer".to_string(),
                columns: vec!["customer_id".to_string()],
                referenced_table: "customers".to_string(),
                referenced_columns: vec!["id".to_string()],
                on_delete: "CASCADE".to_string(),
                on_update: "NO ACTION".to_string(),
            }).unwrap()),
            target_value: None,
            details: "FK needs to be added".to_string(),
            destructive: false,
        };

        // Create a table difference (should come before FK)
        let table_diff = SchemaDifference {
            id: "table-1".to_string(),
            object_type: SchemaObjectType::Table,
            object_name: "public.orders".to_string(),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&Table {
                name: "orders".to_string(),
                schema: "public".to_string(),
                columns: vec![],
                primary_key: None,
                foreign_keys: vec![],
                unique_constraints: vec![],
                check_constraints: vec![],
                indexes: vec![],
                row_count: None,
            }).unwrap()),
            target_value: None,
            details: "Table needs to be added".to_string(),
            destructive: false,
        };

        // Pass FK first, but it should be reordered after table
        let diffs = vec![fk_diff.clone(), table_diff.clone()];
        let operation_ids = vec![fk_diff.id.clone(), table_diff.id.clone()];
        
        let script = generator.generate_script(&diffs, &operation_ids).unwrap();

        // Find positions of CREATE TABLE and ADD CONSTRAINT
        let table_pos = script.find("CREATE TABLE").unwrap();
        let fk_pos = script.find("ADD CONSTRAINT").unwrap();

        // Table should come before FK
        assert!(table_pos < fk_pos, "Table creation should come before foreign key");
    }

    #[test]
    fn test_transaction_boundaries_for_each_database() {
        // PostgreSQL
        let pg_gen = MigrationGenerator::new(DatabaseType::PostgreSQL);
        let diff = create_simple_table_diff();
        let script = pg_gen.generate_script(&[diff.clone()], &[diff.id.clone()]).unwrap();
        assert!(script.starts_with("BEGIN;"));
        assert!(script.ends_with("COMMIT;"));

        // MySQL
        let mysql_gen = MigrationGenerator::new(DatabaseType::MySQL);
        let diff = create_simple_table_diff();
        let script = mysql_gen.generate_script(&[diff.clone()], &[diff.id.clone()]).unwrap();
        assert!(script.starts_with("START TRANSACTION;"));
        assert!(script.ends_with("COMMIT;"));

        // SQLite
        let sqlite_gen = MigrationGenerator::new(DatabaseType::SQLite);
        let diff = create_simple_table_diff();
        let script = sqlite_gen.generate_script(&[diff.clone()], &[diff.id.clone()]).unwrap();
        assert!(script.starts_with("BEGIN;"));
        assert!(script.ends_with("COMMIT;"));

        // SQL Server
        let sqlserver_gen = MigrationGenerator::new(DatabaseType::SQLServer);
        let diff = create_simple_table_diff();
        let script = sqlserver_gen.generate_script(&[diff.clone()], &[diff.id.clone()]).unwrap();
        assert!(script.starts_with("BEGIN TRANSACTION;"));
        assert!(script.ends_with("COMMIT;"));
    }

    #[test]
    fn test_savepoints_for_multiple_operations() {
        let generator = MigrationGenerator::new(DatabaseType::PostgreSQL);
        
        let diffs = vec![
            create_simple_table_diff(),
            create_simple_table_diff(),
            create_simple_table_diff(),
        ];
        let operation_ids: Vec<String> = diffs.iter().map(|d| d.id.clone()).collect();
        
        let script = generator.generate_script(&diffs, &operation_ids).unwrap();

        // Should have 3 savepoints
        assert!(script.contains("SAVEPOINT sp_operation_1"));
        assert!(script.contains("SAVEPOINT sp_operation_2"));
        assert!(script.contains("SAVEPOINT sp_operation_3"));
    }

    #[test]
    fn test_index_creation_all_databases() {
        let index = Index {
            name: "idx_email".to_string(),
            columns: vec!["email".to_string()],
            unique: true,
            index_type: "BTREE".to_string(),
            partial: false,
            condition: None,
        };

        let diff = SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Index,
            object_name: "public.users.idx_email".to_string(),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&index).unwrap()),
            target_value: None,
            details: "Index needs to be added".to_string(),
            destructive: false,
        };

        // PostgreSQL
        let pg_gen = MigrationGenerator::new(DatabaseType::PostgreSQL);
        let script = pg_gen.generate_script(&[diff.clone()], &[diff.id.clone()]).unwrap();
        assert!(script.contains("CREATE UNIQUE INDEX idx_email"));

        // MySQL
        let mysql_gen = MigrationGenerator::new(DatabaseType::MySQL);
        let script = mysql_gen.generate_script(&[diff.clone()], &[diff.id.clone()]).unwrap();
        assert!(script.contains("CREATE UNIQUE INDEX `idx_email`"));

        // SQLite
        let sqlite_gen = MigrationGenerator::new(DatabaseType::SQLite);
        let script = sqlite_gen.generate_script(&[diff.clone()], &[diff.id.clone()]).unwrap();
        assert!(script.contains("CREATE UNIQUE INDEX \"idx_email\""));

        // SQL Server
        let sqlserver_gen = MigrationGenerator::new(DatabaseType::SQLServer);
        let script = sqlserver_gen.generate_script(&[diff.clone()], &[diff.id.clone()]).unwrap();
        assert!(script.contains("CREATE UNIQUE NONCLUSTERED INDEX [idx_email]"));
    }

    // Helper function
    fn create_simple_table_diff() -> SchemaDifference {
        let table = Table {
            name: format!("test_table_{}", Uuid::new_v4().to_string().replace("-", "_")),
            schema: "public".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    auto_increment: false,
                    comment: None,
                    ordinal_position: 1,
                },
            ],
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
            row_count: None,
        };

        SchemaDifference {
            id: Uuid::new_v4().to_string(),
            object_type: SchemaObjectType::Table,
            object_name: format!("public.{}", table.name),
            change_type: ChangeType::Addition,
            source_value: Some(serde_json::to_value(&table).unwrap()),
            target_value: None,
            details: format!("Table {} needs to be added", table.name),
            destructive: false,
        }
    }
}
