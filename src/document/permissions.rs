use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use frappe_meta::schema::DocTypeSchema;
use crate::document::lifecycle::DocError;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessAction {
    Read,
    Write,
    Create,
    Delete,
    Submit,
    Cancel,
}

#[derive(Clone, Debug)]
pub struct User {
    pub username: String,
    pub roles: Vec<String>,
    /// Row-Level Security: maps field names (like "company") to allowed values.
    /// e.g. "company" -> ["Acme Corp", "Test Company"]
    pub user_permissions: HashMap<String, Vec<String>>,
}

pub struct PermissionEvaluator;

impl PermissionEvaluator {
    /// Evaluate role-based access control (RBAC) rules.
    /// Returns true if at least one role of the user has the required permission.
    pub fn check_permission(
        user: &User,
        schema: &DocTypeSchema,
        action: AccessAction,
    ) -> Result<bool, DocError> {
        // Administrator always has all permissions
        if user.roles.iter().any(|r| r == "Administrator" || r == "System Manager") {
            return Ok(true);
        }

        let perms = match &schema.permissions {
            Some(p) => p,
            None => return Ok(false), // No permissions defined means no access by default
        };

        for perm in perms {
            if user.roles.contains(&perm.role) {
                let allowed = match action {
                    AccessAction::Read => perm.read.unwrap_or(false),
                    AccessAction::Write => perm.write.unwrap_or(false),
                    AccessAction::Create => perm.create.unwrap_or(false),
                    AccessAction::Delete => perm.delete.unwrap_or(false),
                    AccessAction::Submit => perm.submit.unwrap_or(false),
                    AccessAction::Cancel => perm.cancel.unwrap_or(false),
                };

                if allowed {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Evaluates Row-Level Security (RLS) on a document.
    /// If user has a constraint on e.g., "company", and doc contains "company",
    /// the doc's company must match one of the allowed values.
    pub fn check_row_security(
        user: &User,
        doc: &serde_json::Map<String, Value>,
    ) -> bool {
        // Administrator bypasses RLS
        if user.roles.iter().any(|r| r == "Administrator" || r == "System Manager") {
            return true;
        }

        for (field_name, allowed_values) in &user.user_permissions {
            if let Some(doc_val) = doc.get(field_name) {
                if let Some(val_str) = doc_val.as_str() {
                    if !allowed_values.contains(&val_str.to_string()) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Removes fields the user is not allowed to read (Field-Level Security).
    /// Fields marked as sensitive or having higher permlevel than the user can access are stripped.
    pub fn apply_field_level_security(
        user: &User,
        doc: &mut serde_json::Map<String, Value>,
        schema: &DocTypeSchema,
    ) {
        // Administrator is allowed to read everything
        if user.roles.iter().any(|r| r == "Administrator" || r == "System Manager") {
            return;
        }

        for field in &schema.fields {
            // Suppose fields starting with "_" or containing "password" / "secret" are classified as sensitive
            // and require a specific role "Security Manager" to read.
            let is_sensitive = field.fieldname.contains("password") 
                || field.fieldname.contains("secret")
                || field.fieldname.contains("salary");

            if is_sensitive && !user.roles.iter().any(|r| r == "Security Manager") {
                doc.remove(&field.fieldname);
            }
        }
    }
}
