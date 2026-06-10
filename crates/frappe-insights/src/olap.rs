use std::path::PathBuf;
use std::sync::Mutex;
use duckdb::{Connection, types::Value as DbValue};
use serde_json::Value as JsonValue;

#[derive(thiserror::Error, Debug)]
pub enum OlapError {
    #[error("Database error: {0}")]
    Database(#[from] duckdb::Error),
    #[error("Security error: {0}")]
    Security(String),
    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),
}

pub struct OlapEngine {
    conn: Mutex<Connection>,
    sandbox_dir: PathBuf,
}

impl OlapEngine {
    /// Creates a new OlapEngine with a validated sandbox directory.
    pub fn new(sandbox_dir: PathBuf) -> Result<Self, OlapError> {
        let conn = Connection::open_in_memory()?;
        let canonical_sandbox = sandbox_dir
            .canonicalize()
            .map_err(|e| OlapError::Security(format!("Failed to canonicalize sandbox directory: {}", e)))?;

        Ok(Self {
            conn: Mutex::new(conn),
            sandbox_dir: canonical_sandbox,
        })
    }

    /// Safely resolves and validates that a file lies strictly within the sandbox directory.
    pub fn validate_and_resolve_path(&self, filename: &str) -> Result<PathBuf, OlapError> {
        // Enforce basic cleanup of path traversal elements
        let clean_name = std::path::Path::new(filename)
            .file_name()
            .ok_or_else(|| OlapError::Security("Invalid filename".to_string()))?
            .to_str()
            .ok_or_else(|| OlapError::Security("Filename contains invalid UTF-8".to_string()))?;

        let target = self.sandbox_dir.join(clean_name);
        let canonical_target = target
            .canonicalize()
            .map_err(|e| OlapError::Security(format!("Path resolution failed: {}", e)))?;

        if !canonical_target.starts_with(&self.sandbox_dir) {
            return Err(OlapError::Security("Directory traversal attempt detected".to_string()));
        }

        Ok(canonical_target)
    }

    /// Registers a local Parquet file as a database view.
    pub fn register_parquet_file(&self, view_name: &str, filename: &str) -> Result<(), OlapError> {
        if !self.validate_identifier(view_name) {
            return Err(OlapError::InvalidIdentifier(format!("'{}' is not a valid SQL identifier", view_name)));
        }

        let resolved_path = self.validate_and_resolve_path(filename)?;
        let path_str = resolved_path
            .to_str()
            .ok_or_else(|| OlapError::Security("Resolved path has invalid characters".to_string()))?;

        // Double check escaping to avoid single-quote injection in filename string
        let escaped_path = path_str.replace('\'', "''");

        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "CREATE OR REPLACE VIEW {} AS SELECT * FROM read_parquet('{}')",
            view_name, escaped_path
        );
        conn.execute(&sql, [])?;
        Ok(())
    }

    /// Executes an analytical SQL query with parameterized values.
    pub fn execute_query(&self, sql: &str, params: &[&dyn duckdb::ToSql]) -> Result<Vec<JsonValue>, OlapError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let col_names = stmt.column_names();
        let mut rows = stmt.query(params)?;

        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            let mut map = serde_json::Map::new();
            for (i, name) in col_names.iter().enumerate() {
                let val: DbValue = row.get(i)?;
                map.insert(name.clone(), self.db_value_to_json(val));
            }
            results.push(JsonValue::Object(map));
        }

        Ok(results)
    }

    fn validate_identifier(&self, name: &str) -> bool {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    fn db_value_to_json(&self, val: DbValue) -> JsonValue {
        match val {
            DbValue::Null => JsonValue::Null,
            DbValue::Boolean(b) => JsonValue::Bool(b),
            DbValue::TinyInt(i) => JsonValue::Number(i.into()),
            DbValue::SmallInt(i) => JsonValue::Number(i.into()),
            DbValue::Int(i) => JsonValue::Number(i.into()),
            DbValue::BigInt(i) => JsonValue::Number(i.into()),
            DbValue::HugeInt(i) => {
                if let Some(num) = serde_json::Number::from_i128(i) {
                    JsonValue::Number(num)
                } else {
                    JsonValue::String(i.to_string())
                }
            }
            DbValue::UTinyInt(u) => JsonValue::Number(u.into()),
            DbValue::USmallInt(u) => JsonValue::Number(u.into()),
            DbValue::UInt(u) => JsonValue::Number(u.into()),
            DbValue::UBigInt(u) => JsonValue::Number(u.into()),
            DbValue::Float(f) => serde_json::Number::from_f64(f as f64).map(JsonValue::Number).unwrap_or(JsonValue::Null),
            DbValue::Double(d) => serde_json::Number::from_f64(d).map(JsonValue::Number).unwrap_or(JsonValue::Null),
            DbValue::Text(s) => JsonValue::String(s),
            DbValue::Blob(b) => JsonValue::String(
                std::str::from_utf8(&b)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>())
            ),
            _ => JsonValue::String(format!("{:?}", val)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_sandbox_path_validation() {
        let dir = tempdir().unwrap();
        let sandbox_path = dir.path().to_path_buf();
        
        // Create a file in sandbox
        let file_path = sandbox_path.join("test.txt");
        File::create(&file_path).unwrap();

        let engine = OlapEngine::new(sandbox_path).unwrap();
        
        // Valid file resolution
        let resolved = engine.validate_and_resolve_path("test.txt");
        assert!(resolved.is_ok());

        // Attempt directory traversal
        let resolved_traversal = engine.validate_and_resolve_path("../test.txt");
        assert!(resolved_traversal.is_ok()); // normalized to test.txt file_name
    }

    #[test]
    fn test_duckdb_basic_queries() {
        let dir = tempdir().unwrap();
        let engine = OlapEngine::new(dir.path().to_path_buf()).unwrap();

        // Run direct setup commands to query against in-memory db
        {
            let conn = engine.conn.lock().unwrap();
            conn.execute("CREATE TABLE sales (id INTEGER, amount DOUBLE, region TEXT)", []).unwrap();
            conn.execute("INSERT INTO sales VALUES (1, 100.50, 'North'), (2, 250.75, 'South')", []).unwrap();
        }

        let results = engine.execute_query(
            "SELECT region, SUM(amount) as total FROM sales GROUP BY region ORDER BY total DESC",
            &[]
        ).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["region"], "South");
        assert_eq!(results[0]["total"], 250.75);
    }
}
