use serde::{Deserialize, Serialize};

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
