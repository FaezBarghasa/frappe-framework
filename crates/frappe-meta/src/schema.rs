use serde::{Deserialize, Serialize};

pub use crate::schema_compiler::{compile_schema_to_surrealql, map_field_type_to_surreal};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocField {
    pub fieldname: String,
    pub fieldtype: String, // e.g. "Data", "Int", "Float", "Check", "Link", "Table", "Text Editor", "Decimal"
    pub label: Option<String>,
    pub reqd: Option<bool>,
    pub unique: Option<bool>,
    pub options: Option<String>, // used for Link target doctype or Select options
    pub default: Option<String>,
    pub allow_on_submit: Option<bool>,
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
    pub is_child_table: Option<bool>,
}
