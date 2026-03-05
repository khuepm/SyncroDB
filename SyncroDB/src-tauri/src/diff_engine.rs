use crate::error::Result;
use crate::models::*;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub struct DiffEngine;

impl DiffEngine {
    pub fn new() -> Self {
        Self
    }

    /// Main comparison function implementing multi-phase algorithm
    pub fn compare_schemas(
        &self,
        source: &DatabaseSchema,
        target: &DatabaseSchema,
    ) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();

        // Phase 1: Object Identification - compare each object type
        differences.extend(self.compare_tables(&source.tables, &target.tables)?);
        differences.extend(self.compare_views(&source.views, &target.views)?);
        differences.extend(self.compare_procedures(&source.procedures, &target.procedures)?);
        differences.extend(self.compare_functions(&source.functions, &target.functions)?);
        differences.extend(self.compare_triggers(&source.triggers, &target.triggers)?);
        differences.extend(self.compare_sequences(&source.sequences, &target.sequences)?);

        // Phase 3: Dependency Analysis and Topological Sorting
        let sorted_differences = self.sort_by_dependencies(differences)?;

        Ok(sorted_differences)
    }

    /// Compare tables and their components
    fn compare_tables(&self, source: &[Table], target: &[Table]) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();

        let source_map: HashMap<String, &Table> = source
            .iter()
            .map(|t| (format!("{}.{}", t.schema, t.name), t))
            .collect();

        let target_map: HashMap<String, &Table> = target
            .iter()
            .map(|t| (format!("{}.{}", t.schema, t.name), t))
            .collect();

        // Identify additions (in source but not in target)
        for (key, table) in &source_map {
            if !target_map.contains_key(key) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::Table,
                    object_name: key.clone(),
                    change_type: ChangeType::Addition,
                    source_value: Some(serde_json::to_value(table).unwrap()),
                    target_value: None,
                    details: format!("Table {} needs to be added", key),
                    destructive: false,
                });
            }
        }

        // Identify deletions (in target but not in source)
        for (key, table) in &target_map {
            if !source_map.contains_key(key) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::Table,
                    object_name: key.clone(),
                    change_type: ChangeType::Deletion,
                    source_value: None,
                    target_value: Some(serde_json::to_value(table).unwrap()),
                    details: format!("Table {} needs to be dropped", key),
                    destructive: true,
                });
            }
        }

        // Identify modifications (in both, compare deeply)
        for (key, source_table) in &source_map {
            if let Some(target_table) = target_map.get(key) {
                // Phase 2: Deep comparison for modified objects
                differences.extend(self.compare_table_details(source_table, target_table)?);
            }
        }

        Ok(differences)
    }

    /// Deep comparison of table details
    fn compare_table_details(
        &self,
        source: &Table,
        target: &Table,
    ) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();
        let table_key = format!("{}.{}", source.schema, source.name);

        // Compare columns
        differences.extend(self.compare_columns(&table_key, &source.columns, &target.columns)?);

        // Compare primary keys
        differences.extend(self.compare_primary_keys(&table_key, &source.primary_key, &target.primary_key)?);

        // Compare foreign keys
        differences.extend(self.compare_foreign_keys(&table_key, &source.foreign_keys, &target.foreign_keys)?);

        // Compare unique constraints
        differences.extend(self.compare_unique_constraints(&table_key, &source.unique_constraints, &target.unique_constraints)?);

        // Compare check constraints
        differences.extend(self.compare_check_constraints(&table_key, &source.check_constraints, &target.check_constraints)?);

        // Compare indexes
        differences.extend(self.compare_indexes(&table_key, &source.indexes, &target.indexes)?);

        Ok(differences)
    }

    /// Compare columns
    fn compare_columns(
        &self,
        table_key: &str,
        source: &[Column],
        target: &[Column],
    ) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();

        let source_map: HashMap<String, &Column> = source
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        let target_map: HashMap<String, &Column> = target
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        // Additions
        for (name, column) in &source_map {
            if !target_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::Column,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Addition,
                    source_value: Some(serde_json::to_value(column).unwrap()),
                    target_value: None,
                    details: format!("Column {} needs to be added to table {}", name, table_key),
                    destructive: false,
                });
            }
        }

        // Deletions
        for (name, column) in &target_map {
            if !source_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::Column,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Deletion,
                    source_value: None,
                    target_value: Some(serde_json::to_value(column).unwrap()),
                    details: format!("Column {} needs to be dropped from table {}", name, table_key),
                    destructive: true,
                });
            }
        }

        // Modifications
        for (name, source_col) in &source_map {
            if let Some(target_col) = target_map.get(name) {
                if !self.columns_equal(source_col, target_col) {
                    let is_destructive = self.is_column_change_destructive(source_col, target_col);
                    differences.push(SchemaDifference {
                        id: Uuid::new_v4().to_string(),
                        object_type: SchemaObjectType::Column,
                        object_name: format!("{}.{}", table_key, name),
                        change_type: ChangeType::Modification,
                        source_value: Some(serde_json::to_value(source_col).unwrap()),
                        target_value: Some(serde_json::to_value(target_col).unwrap()),
                        details: self.get_column_change_details(source_col, target_col),
                        destructive: is_destructive,
                    });
                }
            }
        }

        Ok(differences)
    }

    fn columns_equal(&self, a: &Column, b: &Column) -> bool {
        a.data_type == b.data_type
            && a.nullable == b.nullable
            && a.default_value == b.default_value
            && a.auto_increment == b.auto_increment
    }

    fn is_column_change_destructive(&self, source: &Column, target: &Column) -> bool {
        // Type change is destructive
        if source.data_type != target.data_type {
            return true;
        }
        // Making column NOT NULL when it was nullable is destructive
        if !source.nullable && target.nullable {
            return true;
        }
        false
    }

    fn get_column_change_details(&self, source: &Column, target: &Column) -> String {
        let mut changes = Vec::new();
        
        if source.data_type != target.data_type {
            changes.push(format!("type: {} -> {}", target.data_type, source.data_type));
        }
        if source.nullable != target.nullable {
            changes.push(format!("nullable: {} -> {}", target.nullable, source.nullable));
        }
        if source.default_value != target.default_value {
            changes.push(format!(
                "default: {:?} -> {:?}",
                target.default_value, source.default_value
            ));
        }
        
        format!("Column modified: {}", changes.join(", "))
    }

    /// Compare primary keys
    fn compare_primary_keys(
        &self,
        table_key: &str,
        source: &Option<PrimaryKey>,
        target: &Option<PrimaryKey>,
    ) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();

        match (source, target) {
            (Some(src_pk), None) => {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::PrimaryKey,
                    object_name: format!("{}.{}", table_key, src_pk.name),
                    change_type: ChangeType::Addition,
                    source_value: Some(serde_json::to_value(src_pk).unwrap()),
                    target_value: None,
                    details: format!("Primary key {} needs to be added", src_pk.name),
                    destructive: false,
                });
            }
            (None, Some(tgt_pk)) => {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::PrimaryKey,
                    object_name: format!("{}.{}", table_key, tgt_pk.name),
                    change_type: ChangeType::Deletion,
                    source_value: None,
                    target_value: Some(serde_json::to_value(tgt_pk).unwrap()),
                    details: format!("Primary key {} needs to be dropped", tgt_pk.name),
                    destructive: true,
                });
            }
            (Some(src_pk), Some(tgt_pk)) => {
                if src_pk.columns != tgt_pk.columns {
                    differences.push(SchemaDifference {
                        id: Uuid::new_v4().to_string(),
                        object_type: SchemaObjectType::PrimaryKey,
                        object_name: format!("{}.{}", table_key, src_pk.name),
                        change_type: ChangeType::Modification,
                        source_value: Some(serde_json::to_value(src_pk).unwrap()),
                        target_value: Some(serde_json::to_value(tgt_pk).unwrap()),
                        details: format!(
                            "Primary key columns changed: {:?} -> {:?}",
                            tgt_pk.columns, src_pk.columns
                        ),
                        destructive: true,
                    });
                }
            }
            (None, None) => {}
        }

        Ok(differences)
    }

    /// Compare foreign keys
    fn compare_foreign_keys(
        &self,
        table_key: &str,
        source: &[ForeignKey],
        target: &[ForeignKey],
    ) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();

        let source_map: HashMap<String, &ForeignKey> = source
            .iter()
            .map(|fk| (fk.name.clone(), fk))
            .collect();

        let target_map: HashMap<String, &ForeignKey> = target
            .iter()
            .map(|fk| (fk.name.clone(), fk))
            .collect();

        // Additions
        for (name, fk) in &source_map {
            if !target_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::ForeignKey,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Addition,
                    source_value: Some(serde_json::to_value(fk).unwrap()),
                    target_value: None,
                    details: format!("Foreign key {} needs to be added", name),
                    destructive: false,
                });
            }
        }

        // Deletions
        for (name, fk) in &target_map {
            if !source_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::ForeignKey,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Deletion,
                    source_value: None,
                    target_value: Some(serde_json::to_value(fk).unwrap()),
                    details: format!("Foreign key {} needs to be dropped", name),
                    destructive: true,
                });
            }
        }

        // Modifications
        for (name, src_fk) in &source_map {
            if let Some(tgt_fk) = target_map.get(name) {
                if !self.foreign_keys_equal(src_fk, tgt_fk) {
                    differences.push(SchemaDifference {
                        id: Uuid::new_v4().to_string(),
                        object_type: SchemaObjectType::ForeignKey,
                        object_name: format!("{}.{}", table_key, name),
                        change_type: ChangeType::Modification,
                        source_value: Some(serde_json::to_value(src_fk).unwrap()),
                        target_value: Some(serde_json::to_value(tgt_fk).unwrap()),
                        details: format!("Foreign key {} has been modified", name),
                        destructive: true,
                    });
                }
            }
        }

        Ok(differences)
    }

    fn foreign_keys_equal(&self, a: &ForeignKey, b: &ForeignKey) -> bool {
        a.columns == b.columns
            && a.referenced_table == b.referenced_table
            && a.referenced_columns == b.referenced_columns
            && a.on_delete == b.on_delete
            && a.on_update == b.on_update
    }

    /// Compare unique constraints
    fn compare_unique_constraints(
        &self,
        table_key: &str,
        source: &[UniqueConstraint],
        target: &[UniqueConstraint],
    ) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();

        let source_map: HashMap<String, &UniqueConstraint> = source
            .iter()
            .map(|uc| (uc.name.clone(), uc))
            .collect();

        let target_map: HashMap<String, &UniqueConstraint> = target
            .iter()
            .map(|uc| (uc.name.clone(), uc))
            .collect();

        // Additions
        for (name, uc) in &source_map {
            if !target_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::UniqueConstraint,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Addition,
                    source_value: Some(serde_json::to_value(uc).unwrap()),
                    target_value: None,
                    details: format!("Unique constraint {} needs to be added", name),
                    destructive: false,
                });
            }
        }

        // Deletions
        for (name, uc) in &target_map {
            if !source_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::UniqueConstraint,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Deletion,
                    source_value: None,
                    target_value: Some(serde_json::to_value(uc).unwrap()),
                    details: format!("Unique constraint {} needs to be dropped", name),
                    destructive: true,
                });
            }
        }

        // Modifications
        for (name, src_uc) in &source_map {
            if let Some(tgt_uc) = target_map.get(name) {
                if src_uc.columns != tgt_uc.columns {
                    differences.push(SchemaDifference {
                        id: Uuid::new_v4().to_string(),
                        object_type: SchemaObjectType::UniqueConstraint,
                        object_name: format!("{}.{}", table_key, name),
                        change_type: ChangeType::Modification,
                        source_value: Some(serde_json::to_value(src_uc).unwrap()),
                        target_value: Some(serde_json::to_value(tgt_uc).unwrap()),
                        details: format!("Unique constraint {} has been modified", name),
                        destructive: true,
                    });
                }
            }
        }

        Ok(differences)
    }

    /// Compare check constraints
    fn compare_check_constraints(
        &self,
        table_key: &str,
        source: &[CheckConstraint],
        target: &[CheckConstraint],
    ) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();

        let source_map: HashMap<String, &CheckConstraint> = source
            .iter()
            .map(|cc| (cc.name.clone(), cc))
            .collect();

        let target_map: HashMap<String, &CheckConstraint> = target
            .iter()
            .map(|cc| (cc.name.clone(), cc))
            .collect();

        // Additions
        for (name, cc) in &source_map {
            if !target_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::CheckConstraint,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Addition,
                    source_value: Some(serde_json::to_value(cc).unwrap()),
                    target_value: None,
                    details: format!("Check constraint {} needs to be added", name),
                    destructive: false,
                });
            }
        }

        // Deletions
        for (name, cc) in &target_map {
            if !source_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::CheckConstraint,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Deletion,
                    source_value: None,
                    target_value: Some(serde_json::to_value(cc).unwrap()),
                    details: format!("Check constraint {} needs to be dropped", name),
                    destructive: true,
                });
            }
        }

        // Modifications
        for (name, src_cc) in &source_map {
            if let Some(tgt_cc) = target_map.get(name) {
                if src_cc.expression != tgt_cc.expression {
                    differences.push(SchemaDifference {
                        id: Uuid::new_v4().to_string(),
                        object_type: SchemaObjectType::CheckConstraint,
                        object_name: format!("{}.{}", table_key, name),
                        change_type: ChangeType::Modification,
                        source_value: Some(serde_json::to_value(src_cc).unwrap()),
                        target_value: Some(serde_json::to_value(tgt_cc).unwrap()),
                        details: format!("Check constraint {} has been modified", name),
                        destructive: true,
                    });
                }
            }
        }

        Ok(differences)
    }

    /// Compare indexes
    fn compare_indexes(
        &self,
        table_key: &str,
        source: &[Index],
        target: &[Index],
    ) -> Result<Vec<SchemaDifference>> {
        let mut differences = Vec::new();

        let source_map: HashMap<String, &Index> = source
            .iter()
            .map(|idx| (idx.name.clone(), idx))
            .collect();

        let target_map: HashMap<String, &Index> = target
            .iter()
            .map(|idx| (idx.name.clone(), idx))
            .collect();

        // Additions
        for (name, idx) in &source_map {
            if !target_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::Index,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Addition,
                    source_value: Some(serde_json::to_value(idx).unwrap()),
                    target_value: None,
                    details: format!("Index {} needs to be added", name),
                    destructive: false,
                });
            }
        }

        // Deletions
        for (name, idx) in &target_map {
            if !source_map.contains_key(name) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: SchemaObjectType::Index,
                    object_name: format!("{}.{}", table_key, name),
                    change_type: ChangeType::Deletion,
                    source_value: None,
                    target_value: Some(serde_json::to_value(idx).unwrap()),
                    details: format!("Index {} needs to be dropped", name),
                    destructive: false, // Dropping indexes is generally not destructive
                });
            }
        }

        // Modifications
        for (name, src_idx) in &source_map {
            if let Some(tgt_idx) = target_map.get(name) {
                if !self.indexes_equal(src_idx, tgt_idx) {
                    differences.push(SchemaDifference {
                        id: Uuid::new_v4().to_string(),
                        object_type: SchemaObjectType::Index,
                        object_name: format!("{}.{}", table_key, name),
                        change_type: ChangeType::Modification,
                        source_value: Some(serde_json::to_value(src_idx).unwrap()),
                        target_value: Some(serde_json::to_value(tgt_idx).unwrap()),
                        details: format!("Index {} has been modified", name),
                        destructive: false,
                    });
                }
            }
        }

        Ok(differences)
    }

    fn indexes_equal(&self, a: &Index, b: &Index) -> bool {
        a.columns == b.columns
            && a.unique == b.unique
            && a.index_type == b.index_type
            && a.partial == b.partial
            && a.condition == b.condition
    }

    /// Compare views
    fn compare_views(&self, source: &[View], target: &[View]) -> Result<Vec<SchemaDifference>> {
        self.compare_simple_objects(
            source,
            target,
            SchemaObjectType::View,
            |v| format!("{}.{}", v.schema, v.name),
            |a, b| a.definition == b.definition,
        )
    }

    /// Compare stored procedures
    fn compare_procedures(
        &self,
        source: &[StoredProcedure],
        target: &[StoredProcedure],
    ) -> Result<Vec<SchemaDifference>> {
        self.compare_simple_objects(
            source,
            target,
            SchemaObjectType::Procedure,
            |p| format!("{}.{}", p.schema, p.name),
            |a, b| a.definition == b.definition,
        )
    }

    /// Compare functions
    fn compare_functions(
        &self,
        source: &[Function],
        target: &[Function],
    ) -> Result<Vec<SchemaDifference>> {
        self.compare_simple_objects(
            source,
            target,
            SchemaObjectType::Function,
            |f| format!("{}.{}", f.schema, f.name),
            |a, b| a.definition == b.definition && a.return_type == b.return_type,
        )
    }

    /// Compare triggers
    fn compare_triggers(
        &self,
        source: &[Trigger],
        target: &[Trigger],
    ) -> Result<Vec<SchemaDifference>> {
        self.compare_simple_objects(
            source,
            target,
            SchemaObjectType::Trigger,
            |t| format!("{}.{}", t.table, t.name),
            |a, b| {
                a.definition == b.definition
                    && a.timing == b.timing
                    && a.event == b.event
            },
        )
    }

    /// Compare sequences
    fn compare_sequences(
        &self,
        source: &[Sequence],
        target: &[Sequence],
    ) -> Result<Vec<SchemaDifference>> {
        self.compare_simple_objects(
            source,
            target,
            SchemaObjectType::Sequence,
            |s| format!("{}.{}", s.schema, s.name),
            |a, b| {
                a.start_value == b.start_value
                    && a.increment == b.increment
                    && a.min_value == b.min_value
                    && a.max_value == b.max_value
                    && a.cycle == b.cycle
            },
        )
    }

    /// Generic comparison for simple objects
    fn compare_simple_objects<T, F, E>(
        &self,
        source: &[T],
        target: &[T],
        object_type: SchemaObjectType,
        key_fn: F,
        equal_fn: E,
    ) -> Result<Vec<SchemaDifference>>
    where
        T: serde::Serialize,
        F: Fn(&T) -> String,
        E: Fn(&T, &T) -> bool,
    {
        let mut differences = Vec::new();

        let source_map: HashMap<String, &T> = source
            .iter()
            .map(|obj| (key_fn(obj), obj))
            .collect();

        let target_map: HashMap<String, &T> = target
            .iter()
            .map(|obj| (key_fn(obj), obj))
            .collect();

        // Additions
        for (key, obj) in &source_map {
            if !target_map.contains_key(key) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: object_type.clone(),
                    object_name: key.clone(),
                    change_type: ChangeType::Addition,
                    source_value: Some(serde_json::to_value(obj).unwrap()),
                    target_value: None,
                    details: format!("{:?} {} needs to be added", object_type, key),
                    destructive: false,
                });
            }
        }

        // Deletions
        for (key, obj) in &target_map {
            if !source_map.contains_key(key) {
                differences.push(SchemaDifference {
                    id: Uuid::new_v4().to_string(),
                    object_type: object_type.clone(),
                    object_name: key.clone(),
                    change_type: ChangeType::Deletion,
                    source_value: None,
                    target_value: Some(serde_json::to_value(obj).unwrap()),
                    details: format!("{:?} {} needs to be dropped", object_type, key),
                    destructive: true,
                });
            }
        }

        // Modifications
        for (key, src_obj) in &source_map {
            if let Some(tgt_obj) = target_map.get(key) {
                if !equal_fn(src_obj, tgt_obj) {
                    differences.push(SchemaDifference {
                        id: Uuid::new_v4().to_string(),
                        object_type: object_type.clone(),
                        object_name: key.clone(),
                        change_type: ChangeType::Modification,
                        source_value: Some(serde_json::to_value(src_obj).unwrap()),
                        target_value: Some(serde_json::to_value(tgt_obj).unwrap()),
                        details: format!("{:?} {} has been modified", object_type, key),
                        destructive: true,
                    });
                }
            }
        }

        Ok(differences)
    }

    /// Phase 3: Dependency analysis and topological sorting
    fn sort_by_dependencies(
        &self,
        mut differences: Vec<SchemaDifference>,
    ) -> Result<Vec<SchemaDifference>> {
        // Build dependency graph
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        for diff in &differences {
            in_degree.entry(diff.id.clone()).or_insert(0);
            graph.entry(diff.id.clone()).or_insert_with(Vec::new);
        }

        // Add dependencies based on object types and relationships
        for diff in &differences {
            if let SchemaObjectType::ForeignKey = diff.object_type {
                // Foreign keys depend on tables being created first
                if let Some(source_val) = &diff.source_value {
                    if let Ok(fk) = serde_json::from_value::<ForeignKey>(source_val.clone()) {
                        // Find the referenced table addition
                        for other_diff in &differences {
                            if let SchemaObjectType::Table = other_diff.object_type {
                                if other_diff.object_name.contains(&fk.referenced_table) {
                                    graph.get_mut(&other_diff.id).unwrap().push(diff.id.clone());
                                    *in_degree.get_mut(&diff.id).unwrap() += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Topological sort using Kahn's algorithm
        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut sorted_ids = Vec::new();

        while let Some(id) = queue.pop() {
            sorted_ids.push(id.clone());

            if let Some(neighbors) = graph.get(&id) {
                for neighbor in neighbors {
                    let degree = in_degree.get_mut(neighbor).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push(neighbor.clone());
                    }
                }
            }
        }

        // Check for circular dependencies
        if sorted_ids.len() != differences.len() {
            // Circular dependency detected, return original order
            return Ok(differences);
        }

        // Reorder differences based on sorted IDs
        let diff_map: HashMap<String, SchemaDifference> = differences
            .drain(..)
            .map(|d| (d.id.clone(), d))
            .collect();

        let sorted_differences: Vec<SchemaDifference> = sorted_ids
            .into_iter()
            .filter_map(|id| diff_map.get(&id).cloned())
            .collect();

        Ok(sorted_differences)
    }
}
