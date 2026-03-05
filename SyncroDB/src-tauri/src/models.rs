use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    #[serde(rename = "postgresql")]
    PostgreSQL,
    #[serde(rename = "mysql")]
    MySQL,
    #[serde(rename = "sqlserver")]
    SQLServer,
    #[serde(rename = "sqlite")]
    SQLite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSLConfig {
    pub mode: String,
    pub ca: Option<String>,
    pub cert: Option<String>,
    pub key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConnection {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub db_type: DatabaseType,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl: bool,
    #[serde(rename = "sslConfig")]
    pub ssl_config: Option<SSLConfig>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    pub success: bool,
    pub message: String,
    pub latency: Option<u64>,
    #[serde(rename = "serverVersion")]
    pub server_version: Option<String>,
}

// Schema metadata structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    #[serde(rename = "databaseName")]
    pub database_name: String,
    pub tables: Vec<Table>,
    pub views: Vec<View>,
    pub procedures: Vec<StoredProcedure>,
    pub functions: Vec<Function>,
    pub triggers: Vec<Trigger>,
    pub sequences: Vec<Sequence>,
    #[serde(rename = "analyzedAt")]
    pub analyzed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub schema: String,
    pub columns: Vec<Column>,
    #[serde(rename = "primaryKey")]
    pub primary_key: Option<PrimaryKey>,
    #[serde(rename = "foreignKeys")]
    pub foreign_keys: Vec<ForeignKey>,
    #[serde(rename = "uniqueConstraints")]
    pub unique_constraints: Vec<UniqueConstraint>,
    #[serde(rename = "checkConstraints")]
    pub check_constraints: Vec<CheckConstraint>,
    pub indexes: Vec<Index>,
    #[serde(rename = "rowCount")]
    pub row_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "dataType")]
    pub data_type: String,
    pub nullable: bool,
    #[serde(rename = "defaultValue")]
    pub default_value: Option<String>,
    #[serde(rename = "autoIncrement")]
    pub auto_increment: bool,
    pub comment: Option<String>,
    #[serde(rename = "ordinalPosition")]
    pub ordinal_position: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimaryKey {
    pub name: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(rename = "referencedTable")]
    pub referenced_table: String,
    #[serde(rename = "referencedColumns")]
    pub referenced_columns: Vec<String>,
    #[serde(rename = "onDelete")]
    pub on_delete: String,
    #[serde(rename = "onUpdate")]
    pub on_update: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniqueConstraint {
    pub name: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConstraint {
    pub name: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    #[serde(rename = "type")]
    pub index_type: String,
    pub partial: bool,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    pub schema: String,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProcedure {
    pub name: String,
    pub schema: String,
    pub parameters: Vec<Parameter>,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub schema: String,
    pub parameters: Vec<Parameter>,
    #[serde(rename = "returnType")]
    pub return_type: String,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "dataType")]
    pub data_type: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub name: String,
    pub table: String,
    pub timing: String,
    pub event: String,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sequence {
    pub name: String,
    pub schema: String,
    #[serde(rename = "startValue")]
    pub start_value: i64,
    pub increment: i64,
    #[serde(rename = "minValue")]
    pub min_value: Option<i64>,
    #[serde(rename = "maxValue")]
    pub max_value: Option<i64>,
    pub cycle: bool,
}
