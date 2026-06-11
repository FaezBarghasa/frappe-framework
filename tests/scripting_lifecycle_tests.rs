use serde_json::{Map, Value};
use std::sync::Arc;
use frappe_framework::document::scripting::ScriptSandbox;
use frappe_meta::db::DatabaseClient;
use surrealdb::Surreal;

#[tokio::test]
async fn test_script_blocks_negative_invoice_total() {
    let db = Surreal::init();
    let db_client = Arc::new(DatabaseClient::new(db));
    let sandbox = ScriptSandbox::new(db_client);

    // Document representing a Sales Invoice
    let mut doc = Map::new();
    doc.insert("doctype".to_string(), Value::String("SalesInvoice".to_string()));
    doc.insert("grand_total".to_string(), Value::Number((-100).into()));

    // Script that validates grand_total is non-negative
    let script = r#"
        if doc.grand_total < 0 {
            throw "Grand Total cannot be negative";
        }
    "#;

    let res = sandbox.execute_script(script, doc);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Grand Total cannot be negative"));
}

#[tokio::test]
async fn test_script_infinite_loop_timeout() {
    let db = Surreal::init();
    let db_client = Arc::new(DatabaseClient::new(db));
    let sandbox = ScriptSandbox::new(db_client);

    let doc = Map::new();
    
    // Script with an infinite loop
    let script = r#"
        let x = 0;
        loop {
            x = x + 1;
        }
    "#;

    let res = sandbox.execute_script(script, doc);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("Script execution timeout") || 
        err.contains("Operations limit exceeded") ||
        err.contains("Script Runtime Error")
    );
}
