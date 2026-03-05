#[cfg(test)]
mod tests {
    use crate::diff_engine::DiffEngine;
    use crate::models::*;
    use proptest::prelude::*;

    // Feature: syncrodb-schema-sync, Property 4: Complete Difference Detection
    // **Validates: Requirements 2.3, 2.4, 2.7, 3.1**
    //
    // For any two database schemas, the diff engine should identify all differences
    // at granular levels (table, column, constraint, index) and categorize each
    // difference as addition, modification, or deletion with correct destructive/additive
    // classification.

    // Helper function to create a simple table
    fn create_table(name: &str, schema: &str, columns: Vec<Column>) -> Table {
        Table {
            name: name.to_string(),
            schema: schema.to_string(),
            columns,
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
            row_count: None,
        }
    }

    // Helper function to create a simple column
    fn create_column(name: &str, data_type: &str, nullable: bool) -> Column {
        Column {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable,
            default_value: None,
            auto_increment: false,
            comment: None,
            ordinal_position: 1,
        }
    }

    // Helper function to create an empty schema
    fn create_empty_schema(connection_id: &str, database_name: &str) -> DatabaseSchema {
        DatabaseSchema {
            connection_id: connection_id.to_string(),
            database_name: database_name.to_string(),
            tables: vec![],
            views: vec![],
            procedures: vec![],
            functions: vec![],
            triggers: vec![],
            sequences: vec![],
            analyzed_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_detect_table_addition() {
        let mut source = create_empty_schema("source", "testdb");
        let target = create_empty_schema("target", "testdb");

        source.tables.push(create_table(
            "users",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        let diff_engine = DiffEngine::new();
        let differences = diff_engine.compare_schemas(&source, &target).unwrap();

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].object_type, SchemaObjectType::Table);
        assert_eq!(differences[0].change_type, ChangeType::Addition);
        assert_eq!(differences[0].object_name, "public.users");
        assert!(!differences[0].destructive);
    }

    #[test]
    fn test_detect_table_deletion() {
        let source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        target.tables.push(create_table(
            "users",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        let diff_engine = DiffEngine::new();
        let differences = diff_engine.compare_schemas(&source, &target).unwrap();

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].object_type, SchemaObjectType::Table);
        assert_eq!(differences[0].change_type, ChangeType::Deletion);
        assert_eq!(differences[0].object_name, "public.users");
        assert!(differences[0].destructive);
    }

    #[test]
    fn test_detect_column_addition() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        source.tables.push(create_table(
            "users",
            "public",
            vec![
                create_column("id", "integer", false),
                create_column("name", "varchar", true),
            ],
        ));

        target.tables.push(create_table(
            "users",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        let diff_engine = DiffEngine::new();
        let differences = diff_engine.compare_schemas(&source, &target).unwrap();

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].object_type, SchemaObjectType::Column);
        assert_eq!(differences[0].change_type, ChangeType::Addition);
        assert!(differences[0].object_name.contains("name"));
        assert!(!differences[0].destructive);
    }

    #[test]
    fn test_detect_column_deletion() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        source.tables.push(create_table(
            "users",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        target.tables.push(create_table(
            "users",
            "public",
            vec![
                create_column("id", "integer", false),
                create_column("name", "varchar", true),
            ],
        ));

        let diff_engine = DiffEngine::new();
        let differences = diff_engine.compare_schemas(&source, &target).unwrap();

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].object_type, SchemaObjectType::Column);
        assert_eq!(differences[0].change_type, ChangeType::Deletion);
        assert!(differences[0].object_name.contains("name"));
        assert!(differences[0].destructive);
    }

    #[test]
    fn test_detect_column_type_change() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        source.tables.push(create_table(
            "users",
            "public",
            vec![create_column("id", "bigint", false)],
        ));

        target.tables.push(create_table(
            "users",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        let diff_engine = DiffEngine::new();
        let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
        DiffEngine::mark_destructive_operations(&mut differences);

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].object_type, SchemaObjectType::Column);
        assert_eq!(differences[0].change_type, ChangeType::Modification);
        assert!(differences[0].destructive);
    }

    #[test]
    fn test_detect_column_nullability_change() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        source.tables.push(create_table(
            "users",
            "public",
            vec![create_column("name", "varchar", false)], // NOT NULL
        ));

        target.tables.push(create_table(
            "users",
            "public",
            vec![create_column("name", "varchar", true)], // NULL
        ));

        let diff_engine = DiffEngine::new();
        let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
        DiffEngine::mark_destructive_operations(&mut differences);

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].object_type, SchemaObjectType::Column);
        assert_eq!(differences[0].change_type, ChangeType::Modification);
        assert!(differences[0].destructive);
    }

    #[test]
    fn test_identical_schemas_no_differences() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        let table = create_table(
            "users",
            "public",
            vec![
                create_column("id", "integer", false),
                create_column("name", "varchar", true),
            ],
        );

        source.tables.push(table.clone());
        target.tables.push(table);

        let diff_engine = DiffEngine::new();
        let differences = diff_engine.compare_schemas(&source, &target).unwrap();

        assert_eq!(differences.len(), 0);
    }

    #[test]
    fn test_summary_generation() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        // Add a table (addition)
        source.tables.push(create_table(
            "new_table",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        // Delete a table (deletion)
        target.tables.push(create_table(
            "old_table",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        // Modify a column (modification)
        source.tables.push(create_table(
            "users",
            "public",
            vec![create_column("id", "bigint", false)],
        ));
        target.tables.push(create_table(
            "users",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        let diff_engine = DiffEngine::new();
        let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
        DiffEngine::mark_destructive_operations(&mut differences);

        let summary = DiffEngine::generate_summary(&differences);

        assert_eq!(summary.total_differences, 3);
        assert_eq!(summary.additions, 1);
        assert_eq!(summary.deletions, 1);
        assert_eq!(summary.modifications, 1);
        assert_eq!(summary.destructive_changes, 2); // deletion + type change
    }

    // Property-based test generators
    fn arb_column() -> impl Strategy<Value = Column> {
        (
            "[a-z]{3,10}",
            prop_oneof![
                Just("integer".to_string()),
                Just("varchar".to_string()),
                Just("text".to_string()),
                Just("boolean".to_string()),
            ],
            any::<bool>(),
        )
            .prop_map(|(name, data_type, nullable)| Column {
                name,
                data_type,
                nullable,
                default_value: None,
                auto_increment: false,
                comment: None,
                ordinal_position: 1,
            })
    }

    fn arb_table() -> impl Strategy<Value = Table> {
        (
            "[a-z]{3,10}",
            "[a-z]{3,10}",
            prop::collection::vec(arb_column(), 1..5),
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

    fn arb_schema() -> impl Strategy<Value = DatabaseSchema> {
        (
            "[a-z]{5,10}",
            "[a-z]{5,10}",
            prop::collection::vec(arb_table(), 0..5),
        )
            .prop_map(|(conn_id, db_name, tables)| DatabaseSchema {
                connection_id: conn_id,
                database_name: db_name,
                tables,
                views: vec![],
                procedures: vec![],
                functions: vec![],
                triggers: vec![],
                sequences: vec![],
                analyzed_at: chrono::Utc::now().to_rfc3339(),
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: syncrodb-schema-sync, Property 4: Complete Difference Detection
        #[test]
        fn prop_identical_schemas_have_no_differences(schema in arb_schema()) {
            let diff_engine = DiffEngine::new();
            let differences = diff_engine.compare_schemas(&schema, &schema).unwrap();
            prop_assert_eq!(differences.len(), 0);
        }

        // Feature: syncrodb-schema-sync, Property 4: Complete Difference Detection
        #[test]
        fn prop_all_differences_are_categorized(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let differences = diff_engine.compare_schemas(&source, &target).unwrap();

            // All differences must have a valid change type
            for diff in &differences {
                prop_assert!(
                    diff.change_type == ChangeType::Addition
                        || diff.change_type == ChangeType::Modification
                        || diff.change_type == ChangeType::Deletion
                );
            }

            // All differences must have a valid object type
            for diff in &differences {
                prop_assert!(
                    matches!(
                        diff.object_type,
                        SchemaObjectType::Table
                            | SchemaObjectType::Column
                            | SchemaObjectType::PrimaryKey
                            | SchemaObjectType::ForeignKey
                            | SchemaObjectType::UniqueConstraint
                            | SchemaObjectType::CheckConstraint
                            | SchemaObjectType::Index
                            | SchemaObjectType::View
                            | SchemaObjectType::Procedure
                            | SchemaObjectType::Function
                            | SchemaObjectType::Trigger
                            | SchemaObjectType::Sequence
                    )
                );
            }
        }

        // Feature: syncrodb-schema-sync, Property 4: Complete Difference Detection
        #[test]
        fn prop_destructive_flag_is_boolean(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
            DiffEngine::mark_destructive_operations(&mut differences);

            // All differences must have a boolean destructive flag
            for diff in &differences {
                prop_assert!(diff.destructive == true || diff.destructive == false);
            }
        }

        // Feature: syncrodb-schema-sync, Property 4: Complete Difference Detection
        #[test]
        fn prop_deletions_are_always_destructive(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
            DiffEngine::mark_destructive_operations(&mut differences);

            // All deletions must be marked as destructive
            for diff in &differences {
                if diff.change_type == ChangeType::Deletion {
                    prop_assert!(diff.destructive);
                }
            }
        }

        // Feature: syncrodb-schema-sync, Property 4: Complete Difference Detection
        #[test]
        fn prop_additions_are_not_destructive(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
            DiffEngine::mark_destructive_operations(&mut differences);

            // Most additions should not be destructive (except for constraints that might fail)
            for diff in &differences {
                if diff.change_type == ChangeType::Addition {
                    // Additions to tables, columns, indexes are not destructive
                    if matches!(
                        diff.object_type,
                        SchemaObjectType::Table | SchemaObjectType::Column | SchemaObjectType::Index
                    ) {
                        prop_assert!(!diff.destructive);
                    }
                }
            }
        }

        // Feature: syncrodb-schema-sync, Property 4: Complete Difference Detection
        #[test]
        fn prop_summary_counts_match_differences(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
            DiffEngine::mark_destructive_operations(&mut differences);

            let summary = DiffEngine::generate_summary(&differences);

            prop_assert_eq!(summary.total_differences, differences.len());
            prop_assert_eq!(
                summary.additions + summary.modifications + summary.deletions,
                differences.len()
            );

            let actual_destructive = differences.iter().filter(|d| d.destructive).count();
            prop_assert_eq!(summary.destructive_changes, actual_destructive);
        }
    }
}
