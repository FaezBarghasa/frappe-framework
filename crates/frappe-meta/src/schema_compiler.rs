use surrealdb::Surreal;
use surrealdb::Connection;
use crate::schema::{DocTypeSchema, DocField};

pub struct SchemaCompiler;

impl SchemaCompiler {
    /// Synchronizes a DocType schema by executing its compiled DDL statements against a live SurrealDB instance.
    pub async fn synchronize_schema<C: Connection>(
        db: &Surreal<C>,
        ns: &str,
        database: &str,
        schema: &DocTypeSchema,
    ) -> Result<(), crate::db::MetaError> {
        db.use_ns(ns).use_db(database).await.map_err(|e| crate::db::MetaError::Database(e.to_string()))?;
        
        let ddl_statements = compile_schema_to_surrealql(schema);
        for statement in ddl_statements {
            let res = db.query(&statement).await.map_err(|e| crate::db::MetaError::Database(e.to_string()))?;
            res.check().map_err(|e| crate::db::MetaError::Database(e.to_string()))?;
        }
        
        Ok(())
    }
}

/// Helper function to map Frappe FieldTypes to SurrealDB types.
/// This acts as the Database Mapping Registry.
pub fn map_field_type_to_surreal(field: &DocField) -> &'static str {
    match field.fieldtype.as_str() {
        "Int" => "int",
        "Float" | "Currency" | "Percent" => "float",
        "Decimal" => "decimal",
        "Check" => "bool",
        "Link" => "record",
        "Table" => "array", // Array of nested record IDs / child table references
        "Date" | "Datetime" | "Time" => "datetime",
        _ => "string", // Default to string for Data, Text, Text Editor, Select, etc.
    }
}

/// Compiles a DocType schema to SurrealQL DDL statements for a SCHEMAFULL table.
pub fn compile_schema_to_surrealql(schema: &DocTypeSchema) -> Vec<String> {
    let table_name = &schema.name;
    let mut ddl = vec![
        format!("DEFINE TABLE {} SCHEMAFULL;", table_name),
        // System fields
        format!("DEFINE FIELD creation ON {} TYPE datetime DEFAULT time::now();", table_name),
        format!("DEFINE FIELD modified ON {} TYPE datetime DEFAULT time::now();", table_name),
        format!("DEFINE FIELD modified_by ON {} TYPE string;", table_name),
        format!("DEFINE FIELD owner ON {} TYPE string;", table_name),
        format!("DEFINE FIELD docstatus ON {} TYPE int DEFAULT 0;", table_name),
    ];

    for field in &schema.fields {
        // Skip table/child fields in main table (they are handled as arrays of links or separate tables)
        if field.fieldtype == "Table" {
            ddl.push(format!("DEFINE FIELD {} ON {} TYPE array;", field.fieldname, table_name));
            continue;
        }

        let field_type = map_field_type_to_surreal(field);
        let assert_clause = if field.reqd.unwrap_or(false) {
            " ASSERT $value != NONE"
        } else {
            ""
        };

        ddl.push(format!(
            "DEFINE FIELD {} ON {} TYPE {}{};",
            field.fieldname, table_name, field_type, assert_clause
        ));

        if field.unique.unwrap_or(false) {
            ddl.push(format!(
                "DEFINE INDEX {}_unique ON {} COLUMNS {} UNIQUE;",
                field.fieldname, table_name, field.fieldname
            ));
        }
    }

    ddl
}
