use frappe_meta::schema::{DocTypeSchema, DocField, compile_schema_to_surrealql};
use frappe_meta::migration::SchemaManager;
use frappe_meta::db::DatabaseClient;
use std::collections::HashMap;
use serde_json::json;

#[test]
fn test_schema_ddl_compilation() {
    let schema = DocTypeSchema {
        name: "TestDoc".to_string(),
        is_submittable: Some(false),
        permissions: None,
        fields: vec![
            DocField {
                fieldname: "title".to_string(),
                fieldtype: "Data".to_string(),
                label: Some("Title".to_string()),
                reqd: Some(true),
                unique: Some(true),
                options: None,
                default: None,
            },
            DocField {
                fieldname: "quantity".to_string(),
                fieldtype: "Int".to_string(),
                label: Some("Quantity".to_string()),
                reqd: None,
                unique: None,
                options: None,
                default: None,
            },
        ],
    };

    let ddl = compile_schema_to_surrealql(&schema);
    
    assert!(ddl.contains(&"DEFINE TABLE TestDoc SCHEMAFULL;".to_string()));
    assert!(ddl.contains(&"DEFINE FIELD title ON TestDoc TYPE string ASSERT $value != NONE;".to_string()));
    assert!(ddl.contains(&"DEFINE INDEX title_unique ON TestDoc COLUMNS title UNIQUE;".to_string()));
    assert!(ddl.contains(&"DEFINE FIELD quantity ON TestDoc TYPE int;".to_string()));
}

#[tokio::test]
async fn test_db_operations_mocked_or_skipped() {
    use surrealdb::engine::remote::ws::Ws;
    use surrealdb::Surreal;

    // Attempt to connect to a running SurrealDB instance, skip test if not running.
    let db_res = Surreal::new::<Ws>("127.0.0.1:8000").await;
    let db = match db_res {
        Ok(db) => db,
        Err(_) => {
            println!("SurrealDB not running at 127.0.0.1:8000, skipping integration test.");
            return;
        }
    };

    // Signin
    if let Err(e) = db.signin(surrealdb::opt::auth::Root {
        username: "root".to_string(),
        password: "root".to_string(),
    }).await {
        println!("Failed to signin: {}, skipping database test.", e);
        return;
    }

    // Set namespace and database
    if let Err(e) = db.use_ns("test_ns").use_db("test_db").await {
        println!("Failed to use namespace/db: {}, skipping database test.", e);
        return;
    }

    let schema = DocTypeSchema {
        name: "TaskDoc".to_string(),
        is_submittable: Some(false),
        permissions: None,
        fields: vec![
            DocField {
                fieldname: "description".to_string(),
                fieldtype: "Data".to_string(),
                label: Some("Description".to_string()),
                reqd: Some(true),
                unique: None,
                options: None,
                default: None,
            },
            DocField {
                fieldname: "status".to_string(),
                fieldtype: "Data".to_string(),
                label: Some("Status".to_string()),
                reqd: None,
                unique: None,
                options: None,
                default: None,
            },
        ],
    };

    let manager = SchemaManager::new(&db);
    assert!(manager.sync_schema(&schema).await.is_ok());

    let client = DatabaseClient::new(db);
    
    // Insert document
    let mut doc = serde_json::Map::new();
    doc.insert("name".to_string(), json!("task_1"));
    doc.insert("description".to_string(), json!("Implement Phase 1.1"));
    doc.insert("status".to_string(), json!("Open"));

    let inserted = client.insert_doc("TaskDoc", doc).await.unwrap();
    assert_eq!(inserted.get("description").unwrap().as_str().unwrap(), "Implement Phase 1.1");

    // Update document
    let mut updates = serde_json::Map::new();
    updates.insert("status".to_string(), json!("Completed"));

    let updated = client.update_doc("TaskDoc", "task_1", updates).await.unwrap();
    assert_eq!(updated.get("status").unwrap().as_str().unwrap(), "Completed");

    // Get list
    let mut filters = HashMap::new();
    filters.insert("status".to_string(), json!("Completed"));

    let list = client.get_list("TaskDoc", filters, vec!["description", "status"], 10, 0).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].get("description").unwrap().as_str().unwrap(), "Implement Phase 1.1");
}
