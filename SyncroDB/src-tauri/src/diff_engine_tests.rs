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
}
