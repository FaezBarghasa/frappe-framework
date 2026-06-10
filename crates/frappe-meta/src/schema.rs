use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocField {
    pub fieldname: String,
    pub fieldtype: String, // e.g. "Data", "Int", "Float", "Check", "Link", "Table", "Text Editor"
    pub label: Option<String>,
    pub reqd: Option<bool>,
    pub unique: Option<bool>,
    pub options: Option<String>, // used for Link target doctype or Select options
    pub default: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocPerm {
    pub role: String,
    pub read: Option<bool>,
    pub write: Option<bool>,
    pub create: Option<bool>,
    pub delete: Option<bool>,
    pub submit: Option<bool>,
    pub cancel: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocTypeSchema {
    pub name: String,
    pub fields: Vec<DocField>,
    pub is_submittable: Option<bool>,
    pub permissions: Option<Vec<DocPerm>>,
}


/// Helper function to map Frappe FieldTypes to SurrealDB types.
/// This acts as the Database Mapping Registry.
pub fn map_field_type_to_surreal(field: &DocField) -> &'static str {
    match field.fieldtype.as_str() {
        "Int" => "int",
        "Float" | "Currency" => "float",
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
