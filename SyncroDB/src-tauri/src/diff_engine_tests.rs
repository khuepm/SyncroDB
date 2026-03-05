#[cfg(test)]
mod tests {
    use crate::diff_engine::DiffEngine;
    use crate::models::*;
    use proptest::prelude::*;

    // Helper functions
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

    // Unit tests
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
    fn test_summary_generation() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        source.tables.push(create_table(
            "new_table",
            "public",
            vec![create_column("id", "integer", false)],
        ));

        target.tables.push(create_table(
            "old_table",
            "public",
            vec![create_column("id", "integer", false)],
        ));

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
        assert_eq!(summary.destructive_changes, 2);
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

        // Property 4: Complete Difference Detection
        #[test]
        fn prop_identical_schemas_have_no_differences(schema in arb_schema()) {
            let diff_engine = DiffEngine::new();
            let differences = diff_engine.compare_schemas(&schema, &schema).unwrap();
            prop_assert_eq!(differences.len(), 0);
        }

        // Property 4: Complete Difference Detection
        #[test]
        fn prop_deletions_are_always_destructive(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
            DiffEngine::mark_destructive_operations(&mut differences);

            for diff in &differences {
                if diff.change_type == ChangeType::Deletion {
                    prop_assert!(diff.destructive);
                }
            }
        }

        // Property 5: Difference Report Generation
        #[test]
        fn prop_report_has_valid_summary(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
            DiffEngine::mark_destructive_operations(&mut differences);

            let summary = DiffEngine::generate_summary(&differences);

            prop_assert_eq!(
                summary.total_differences,
                summary.additions + summary.modifications + summary.deletions
            );
            prop_assert!(summary.destructive_changes <= summary.total_differences);
        }

        // Property 5: Difference Report Generation
        #[test]
        fn prop_all_differences_have_required_fields(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let differences = diff_engine.compare_schemas(&source, &target).unwrap();

            for diff in &differences {
                prop_assert!(!diff.id.is_empty());
                prop_assert!(!diff.object_name.is_empty());
                prop_assert!(!diff.details.is_empty());
            }
        }

        // Property 5: Difference Report Generation
        #[test]
        fn prop_difference_ids_are_unique(
            source in arb_schema(),
            target in arb_schema()
        ) {
            let diff_engine = DiffEngine::new();
            let differences = diff_engine.compare_schemas(&source, &target).unwrap();

            let mut seen_ids = std::collections::HashSet::new();
            for diff in &differences {
                prop_assert!(
                    seen_ids.insert(diff.id.clone()),
                    "Duplicate difference ID found: {}",
                    diff.id
                );
            }
        }
    }

    // Additional unit tests for task 4.6
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
    fn test_detect_dropped_column() {
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
        assert!(differences[0].destructive);
    }

    #[test]
    fn test_detect_modified_constraint() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        let mut source_table = create_table("users", "public", vec![create_column("id", "integer", false)]);
        source_table.primary_key = Some(PrimaryKey {
            name: "users_pkey".to_string(),
            columns: vec!["id".to_string()],
        });

        let mut target_table = create_table("users", "public", vec![create_column("id", "integer", false)]);
        target_table.primary_key = Some(PrimaryKey {
            name: "users_pkey".to_string(),
            columns: vec!["id".to_string(), "email".to_string()],
        });

        source.tables.push(source_table);
        target.tables.push(target_table);

        let diff_engine = DiffEngine::new();
        let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
        DiffEngine::mark_destructive_operations(&mut differences);

        assert!(differences.len() > 0);
        let pk_diff = differences.iter().find(|d| d.object_type == SchemaObjectType::PrimaryKey);
        assert!(pk_diff.is_some());
        assert!(pk_diff.unwrap().destructive);
    }

    #[test]
    fn test_empty_schemas() {
        let source = create_empty_schema("source", "testdb");
        let target = create_empty_schema("target", "testdb");

        let diff_engine = DiffEngine::new();
        let differences = diff_engine.compare_schemas(&source, &target).unwrap();

        assert_eq!(differences.len(), 0);
    }

    #[test]
    fn test_identical_schemas() {
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
    fn test_circular_dependency_detection() {
        // This test verifies that the topological sort handles circular dependencies gracefully
        let mut source = create_empty_schema("source", "testdb");
        let target = create_empty_schema("target", "testdb");

        // Create tables with potential circular foreign key dependencies
        let mut table_a = create_table("table_a", "public", vec![create_column("id", "integer", false)]);
        table_a.foreign_keys.push(ForeignKey {
            name: "fk_a_to_b".to_string(),
            columns: vec!["b_id".to_string()],
            referenced_table: "table_b".to_string(),
            referenced_columns: vec!["id".to_string()],
            on_delete: "CASCADE".to_string(),
            on_update: "CASCADE".to_string(),
        });

        let mut table_b = create_table("table_b", "public", vec![create_column("id", "integer", false)]);
        table_b.foreign_keys.push(ForeignKey {
            name: "fk_b_to_a".to_string(),
            columns: vec!["a_id".to_string()],
            referenced_table: "table_a".to_string(),
            referenced_columns: vec!["id".to_string()],
            on_delete: "CASCADE".to_string(),
            on_update: "CASCADE".to_string(),
        });

        source.tables.push(table_a);
        source.tables.push(table_b);

        let diff_engine = DiffEngine::new();
        let result = diff_engine.compare_schemas(&source, &target);

        // Should not panic or error, even with circular dependencies
        assert!(result.is_ok());
    }

    #[test]
    fn test_index_addition_not_destructive() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        let mut source_table = create_table("users", "public", vec![create_column("id", "integer", false)]);
        source_table.indexes.push(Index {
            name: "idx_users_id".to_string(),
            columns: vec!["id".to_string()],
            unique: false,
            index_type: "btree".to_string(),
            partial: false,
            condition: None,
        });

        let target_table = create_table("users", "public", vec![create_column("id", "integer", false)]);

        source.tables.push(source_table);
        target.tables.push(target_table);

        let diff_engine = DiffEngine::new();
        let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
        DiffEngine::mark_destructive_operations(&mut differences);

        let index_diff = differences.iter().find(|d| d.object_type == SchemaObjectType::Index);
        assert!(index_diff.is_some());
        assert!(!index_diff.unwrap().destructive);
    }

    #[test]
    fn test_index_deletion_not_destructive() {
        let mut source = create_empty_schema("source", "testdb");
        let mut target = create_empty_schema("target", "testdb");

        let source_table = create_table("users", "public", vec![create_column("id", "integer", false)]);

        let mut target_table = create_table("users", "public", vec![create_column("id", "integer", false)]);
        target_table.indexes.push(Index {
            name: "idx_users_id".to_string(),
            columns: vec!["id".to_string()],
            unique: false,
            index_type: "btree".to_string(),
            partial: false,
            condition: None,
        });

        source.tables.push(source_table);
        target.tables.push(target_table);

        let diff_engine = DiffEngine::new();
        let mut differences = diff_engine.compare_schemas(&source, &target).unwrap();
        DiffEngine::mark_destructive_operations(&mut differences);

        let index_diff = differences.iter().find(|d| d.object_type == SchemaObjectType::Index);
        assert!(index_diff.is_some());
        assert!(!index_diff.unwrap().destructive);
    }

    #[test]
    fn test_column_type_change_is_destructive() {
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

        let col_diff = differences.iter().find(|d| d.object_type == SchemaObjectType::Column);
        assert!(col_diff.is_some());
        assert!(col_diff.unwrap().destructive);
    }

    #[test]
    fn test_column_nullability_change_is_destructive() {
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

        let col_diff = differences.iter().find(|d| d.object_type == SchemaObjectType::Column);
        assert!(col_diff.is_some());
        assert!(col_diff.unwrap().destructive);
    }
}
