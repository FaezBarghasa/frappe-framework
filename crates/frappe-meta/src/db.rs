use serde_json::{Map, Value};
use std::collections::HashMap;
use thiserror::Error;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;

#[derive(Error, Debug)]
pub enum MetaError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Serialization/Deserialization error: {0}")]
    Serialization(String),
    #[error("Invalid Doctype name or field: {0}")]
    InvalidIdentifier(String),
    #[error("Document not found: {0}")]
    NotFound(String),
}

pub struct DatabaseClient {
    db: Surreal<Client>,
}

impl DatabaseClient {
    pub fn new(db: Surreal<Client>) -> Self {
        Self { db }
    }

    /// Sanitizes identifiers to prevent injection
    fn sanitize_identifier(&self, id: &str) -> Result<String, MetaError> {
        if id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !id.is_empty() {
            Ok(id.to_string())
        } else {
            Err(MetaError::InvalidIdentifier(format!("Identifier contains invalid characters: {}", id)))
        }
    }

    /// Retrieve a list of documents with filters, limits, and offsets.
    pub async fn get_list(
        &self,
        doctype: &str,
        filters: HashMap<String, Value>,
        fields: Vec<&str>,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<Map<String, Value>>, MetaError> {
        let sanitized_tb = self.sanitize_identifier(doctype)?;
        
        let fields_str = if fields.is_empty() {
            "*".to_string()
        } else {
            fields.iter()
                .map(|f| self.sanitize_identifier(f))
                .collect::<Result<Vec<String>, MetaError>>()?
                .join(", ")
        };

        let mut query_str = format!("SELECT {} FROM {}", fields_str, sanitized_tb);

        if !filters.is_empty() {
            let mut where_clauses = Vec::new();
            for (key, _) in &filters {
                let clean_key = self.sanitize_identifier(key)?;
                where_clauses.push(format!("{} = ${}", clean_key, clean_key));
            }
            query_str.push_str(" WHERE ");
            query_str.push_str(&where_clauses.join(" AND "));
        }

        query_str.push_str(" LIMIT $limit START $offset;");

        let mut query = self.db.query(&query_str);

        // Bind filter values
        for (key, val) in filters {
            query = query.bind((key, val));
        }

        // Bind paging parameters
        query = query.bind(("limit", limit)).bind(("offset", offset));

        let mut response = query.await.map_err(|e| MetaError::Database(e.to_string()))?;
        let results: Vec<Value> = response.take(0usize).map_err(|e| MetaError::Database(e.to_string()))?;

        let mut maps = Vec::new();
        for val in results {
            if let Value::Object(map) = val {
                maps.push(map);
            } else {
                return Err(MetaError::Serialization("Expected query to return a list of objects".to_string()));
            }
        }

        Ok(maps)
    }

    /// Insert a new document with dynamic payload.
    pub async fn insert_doc(
        &self,
        doctype: &str,
        mut doc: Map<String, Value>,
    ) -> Result<Map<String, Value>, MetaError> {
        let sanitized_tb = self.sanitize_identifier(doctype)?;

        // Ensure system fields are present
        let now = chrono::Utc::now().to_rfc3339();
        doc.insert("creation".to_string(), Value::String(now.clone()));
        doc.insert("modified".to_string(), Value::String(now));
        if !doc.contains_key("docstatus") {
            doc.insert("docstatus".to_string(), Value::Number(0.into()));
        }

        // Construct document name/ID if it exists, otherwise generate UUID
        let doc_id = doc.get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                let generated = uuid::Uuid::new_v4().to_string();
                doc.insert("name".to_string(), Value::String(generated.clone()));
                generated
            });

        // Safe record ID in SurrealDB
        let record_id = (sanitized_tb, doc_id);

        let created: Option<Value> = self.db.create(record_id)
            .content(Value::Object(doc))
            .await
            .map_err(|e| MetaError::Database(e.to_string()))?;

        match created {
            Some(Value::Object(map)) => Ok(map),
            Some(_) => Err(MetaError::Serialization("Created document is not an object".to_string())),
            None => Err(MetaError::Database("Failed to create document".to_string())),
        }
    }

    /// Update an existing document.
    pub async fn update_doc(
        &self,
        doctype: &str,
        doc_name: &str,
        mut updates: Map<String, Value>,
    ) -> Result<Map<String, Value>, MetaError> {
        let sanitized_tb = self.sanitize_identifier(doctype)?;

        // Update modification timestamp
        let now = chrono::Utc::now().to_rfc3339();
        updates.insert("modified".to_string(), Value::String(now));

        let record_id = (sanitized_tb, doc_name.to_string());

        let updated: Option<Value> = self.db.update(record_id)
            .merge(Value::Object(updates))
            .await
            .map_err(|e| MetaError::Database(e.to_string()))?;

        match updated {
            Some(Value::Object(map)) => Ok(map),
            Some(_) => Err(MetaError::Serialization("Updated document is not an object".to_string())),
            None => Err(MetaError::NotFound(format!("Document {}:{} not found to update", doctype, doc_name))),
        }
    }
}
