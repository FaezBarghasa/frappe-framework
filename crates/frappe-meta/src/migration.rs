use crate::schema::{DocTypeSchema, compile_schema_to_surrealql, map_field_type_to_surreal};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TableInfo {
    #[serde(alias = "fd")]
    pub fields: HashMap<String, String>,
    #[serde(alias = "ix")]
    pub indexes: HashMap<String, String>,
}

pub struct SchemaManager<'a> {
    db: &'a Surreal<Client>,
}

impl<'a> SchemaManager<'a> {
    pub fn new(db: &'a Surreal<Client>) -> Self {
        Self { db }
    }

    /// Synchronize the database schema for the given DocType.
    /// This performs an auto-migration / incremental alter schema.
    pub async fn sync_schema(&self, schema: &DocTypeSchema) -> Result<(), String> {
        let table_name = &schema.name;

        // Ensure table name is safe to prevent query injection in DDL.
        if !table_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("Invalid characters in table name: {}", table_name));
        }

        // Try to get info for the table.
        let info_query = format!("INFO FOR TABLE {};", table_name);
        let info_result = self.db.query(&info_query).await;

        match info_result {
            Ok(mut response) => {
                // Check if table exists. If table doesn't exist, INFO FOR TABLE returns an error or empty result.
                let raw_val: Option<serde_json::Value> = response.take(0usize).unwrap_or(None);
                let table_info: Option<TableInfo> = raw_val.and_then(|v| serde_json::from_value(v).ok());

                if let Some(info) = table_info {
                    // Table exists. Perform incremental migration (Alter Schema).
                    self.alter_table(schema, &info).await?;
                } else {
                    // Table doesn't exist, run full create.
                    self.create_table(schema).await?;
                }
            }
            Err(_) => {
                // INFO FOR TABLE failed (usually means table doesn't exist).
                // Let's attempt to create the table.
                self.create_table(schema).await?;
            }
        }

        Ok(())
    }

    /// Creates a table and defines all its fields in a transaction.
    async fn create_table(&self, schema: &DocTypeSchema) -> Result<(), String> {
        let ddl_queries = compile_schema_to_surrealql(schema);

        let mut query = String::from("BEGIN TRANSACTION;\n");
        for q in ddl_queries {
            query.push_str(&q);
            query.push('\n');
        }
        query.push_str("COMMIT TRANSACTION;\n");

        match self.db.query(&query).await {
            Ok(response) => {
                response.check().map_err(|e| format!("Create table transaction failed: {}", e))?;
                Ok(())
            }
            Err(e) => Err(format!("Create table execution failed: {}", e)),
        }
    }

    /// Alters an existing table by adding/updating required fields and removing dropped fields.
    async fn alter_table(&self, schema: &DocTypeSchema, existing: &TableInfo) -> Result<(), String> {
        let table_name = &schema.name;
        let mut ddl = Vec::new();

        // 1. Identify fields to add or update
        let mut active_fields = std::collections::HashSet::new();
        active_fields.insert("creation".to_string());
        active_fields.insert("modified".to_string());
        active_fields.insert("modified_by".to_string());
        active_fields.insert("owner".to_string());
        active_fields.insert("docstatus".to_string());

        for field in &schema.fields {
            active_fields.insert(field.fieldname.clone());

            let field_type = map_field_type_to_surreal(field);
            let assert_clause = if field.reqd.unwrap_or(false) {
                " ASSERT $value != NONE"
            } else {
                ""
            };

            // We define/re-define the field (SurrealDB DEFINE FIELD is idempotent/updates existing).
            ddl.push(format!(
                "DEFINE FIELD {} ON {} TYPE {}{};",
                field.fieldname, table_name, field_type, assert_clause
            ));

            if field.unique.unwrap_or(false) {
                // Define unique index if it doesn't exist
                let idx_name = format!("{}_unique", field.fieldname);
                if !existing.indexes.contains_key(&idx_name) {
                    ddl.push(format!(
                        "DEFINE INDEX {} ON {} COLUMNS {} UNIQUE;",
                        idx_name, table_name, field.fieldname
                    ));
                }
            }
        }

        // 2. Identify fields to remove (exist in DB but not in schema or system fields)
        for db_field in existing.fields.keys() {
            if !active_fields.contains(db_field) {
                ddl.push(format!("REMOVE FIELD {} ON {};", db_field, table_name));
            }
        }

        if ddl.is_empty() {
            return Ok(());
        }

        // Execute in transaction
        let mut query = String::from("BEGIN TRANSACTION;\n");
        for q in ddl {
            query.push_str(&q);
            query.push('\n');
        }
        query.push_str("COMMIT TRANSACTION;\n");

        match self.db.query(&query).await {
            Ok(response) => {
                response.check().map_err(|e| format!("Alter table transaction failed: {}", e))?;
                Ok(())
            }
            Err(e) => Err(format!("Alter table execution failed: {}", e)),
        }
    }
}
