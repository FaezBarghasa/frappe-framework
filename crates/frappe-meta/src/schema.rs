use serde::{Deserialize, Serialize};
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocField {
    pub fieldname: String,
    pub fieldtype: String, // e.g. "Data", "Int", "Link", "Table"
    pub label: Option<String>,
    pub reqd: Option<bool>,
    pub options: Option<String>, // used for Link target doctype or Select options
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocTypeSchema {
    pub name: String,
    pub fields: Vec<DocField>,
    pub is_submittable: Option<bool>,
}

/// Helper function to compile fields to SurrealQL DDL statements.
pub fn compile_schema_to_surrealql(schema: &DocTypeSchema) -> Vec<String> {
    let table_name = &schema.name;
    let mut ddl = vec![
        format!("DEFINE TABLE {} SCHEMALESS;", table_name),
    ];

    for field in &schema.fields {
        let field_name = &field.fieldname;
        let field_type = match field.fieldtype.as_str() {
            "Int" => "int",
            "Float" => "float",
            "Check" => "bool",
            "Link" => "record",
            "Table" => "array", // Array of nested record IDs
            _ => "string", // Default to string for Data, Text, etc.
        };

        let assert_clause = if field.reqd.unwrap_or(false) {
            " ASSERT $value != NONE"
        } else {
            ""
        };

        ddl.push(format!(
            "DEFINE FIELD {} ON {} TYPE {}{};",
            field_name, table_name, field_type, assert_clause
        ));
    }

    ddl
}

/// Execute dynamic DDL queries against SurrealDB.
pub async fn execute_schema_ddl(db: &Surreal<Client>, schema: &DocTypeSchema) -> Result<(), String> {
    let ddl_queries = compile_schema_to_surrealql(schema);

    // Execute the dynamic queries against the connected SurrealDB instance inside an administrative transaction scope.
    let mut query = String::from("BEGIN TRANSACTION;\n");
    for q in ddl_queries {
        query.push_str(&q);
        query.push('\n');
    }
    query.push_str("COMMIT TRANSACTION;\n");

    match db.query(&query).await {
        Ok(response) => {
            if let Err(e) = response.check() {
                return Err(format!("Transaction failed: {}", e));
            }
            Ok(())
        }
        Err(e) => Err(format!("Query execution failed: {}", e)),
    }
}
