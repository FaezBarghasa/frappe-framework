use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use frappe_meta::schema::DocTypeSchema;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DocError {
    #[error("ValidationError: {0}")]
    Validation(String),
    #[error("TransitionError: Cannot change status from {from:?} to {to:?}")]
    InvalidTransition { from: DocStatus, to: DocStatus },
    #[error("ImmutableFieldError: Field '{field}' cannot be updated after submission")]
    ImmutableFieldUpdate { field: String },
    #[error("CancelledDocumentError: Cancelled documents cannot be modified")]
    ModifiedCancelled,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DocStatus {
    Draft = 0,
    Submitted = 1,
    Cancelled = 2,
}

impl DocStatus {
    pub fn from_i32(val: i32) -> Self {
        match val {
            1 => DocStatus::Submitted,
            2 => DocStatus::Cancelled,
            _ => DocStatus::Draft,
        }
    }
}

#[async_trait]
pub trait DocumentLifecycle {
    async fn before_insert(&mut self) -> Result<(), DocError>;
    async fn validate(&mut self) -> Result<(), DocError>;
    async fn on_update(&mut self) -> Result<(), DocError>;
    async fn on_submit(&mut self) -> Result<(), DocError>;
    async fn on_cancel(&mut self) -> Result<(), DocError>;
}

pub struct DocLifecycleController;

impl DocLifecycleController {
    /// Runs a custom scripting hook on the document before saving it.
    pub fn run_before_save(
        sandbox: &crate::document::scripting::ScriptSandbox,
        script: &str,
        doc: serde_json::Map<String, Value>,
    ) -> Result<serde_json::Map<String, Value>, DocError> {
        sandbox.execute_script(script, doc).map_err(|e| DocError::Validation(e))
    }

    /// Executes submitting operations/queries atomically in SurrealDB.
    pub async fn run_on_submit<C: surrealdb::Connection>(
        db: &surrealdb::Surreal<C>,
        ns: &str,
        database: &str,
        queries: &[String],
    ) -> Result<(), DocError> {
        if queries.is_empty() {
            return Ok(());
        }

        db.use_ns(ns).use_db(database).await.map_err(|e| DocError::Validation(e.to_string()))?;

        let mut tx_query = "BEGIN TRANSACTION;\n".to_string();
        for q in queries {
            tx_query.push_str(q);
            tx_query.push_str("\n");
        }
        tx_query.push_str("COMMIT TRANSACTION;");

        let res = db.query(&tx_query).await.map_err(|e| DocError::Validation(e.to_string()))?;
        res.check().map_err(|e| DocError::Validation(e.to_string()))?;

        Ok(())
    }

    /// Validates the transition between the old document and new document states.
    /// Ensures status moves correctly, fields are not mutated post-submission unless allowed, and cancelled docs are locked.
    pub fn validate_transition(
        old_doc: &serde_json::Map<String, Value>,
        new_doc: &serde_json::Map<String, Value>,
        schema: &DocTypeSchema,
    ) -> Result<(), DocError> {
        let old_status_val = old_doc.get("docstatus")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let new_status_val = new_doc.get("docstatus")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        let old_status = DocStatus::from_i32(old_status_val);
        let new_status = DocStatus::from_i32(new_status_val);

        // 1. Validate status transitions
        match (old_status, new_status) {
            (DocStatus::Draft, DocStatus::Draft) => {}
            (DocStatus::Draft, DocStatus::Submitted) => {}
            (DocStatus::Submitted, DocStatus::Submitted) => {}
            (DocStatus::Submitted, DocStatus::Cancelled) => {}
            (DocStatus::Cancelled, DocStatus::Cancelled) => {}
            _ => {
                return Err(DocError::InvalidTransition {
                    from: old_status,
                    to: new_status,
                });
            }
        }

        // 2. Lock down cancelled documents completely
        if old_status == DocStatus::Cancelled && new_doc != old_doc {
            return Err(DocError::ModifiedCancelled);
        }

        // 3. Prevent modifications to non-allow_on_submit fields in submitted documents
        if old_status == DocStatus::Submitted && new_status == DocStatus::Submitted {
            // Check all schema fields to see if they were modified
            for field in &schema.fields {
                let old_val = old_doc.get(&field.fieldname).unwrap_or(&Value::Null);
                let new_val = new_doc.get(&field.fieldname).unwrap_or(&Value::Null);

                if old_val != new_val {
                    // Check if field type options/metadata allows modification post-submit
                    let allow_on_submit = field.fieldname == "modified" 
                        || field.fieldname == "modified_by"
                        || field.allow_on_submit.unwrap_or(false); // check schema allow_on_submit field

                    if !allow_on_submit {
                        return Err(DocError::ImmutableFieldUpdate {
                            field: field.fieldname.clone(),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}
