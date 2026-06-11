use frappe_meta::schema::{DocField, DocTypeSchema};
use frappe_meta::schema_compiler::compile_schema_to_surrealql;

#[test]
fn test_sales_invoice_ddl_compilation() {
    let schema = DocTypeSchema {
        name: "SalesInvoice".to_string(),
        is_submittable: Some(true),
        permissions: None,
        is_child_table: Some(false),
        fields: vec![
            DocField {
                fieldname: "customer".to_string(),
                fieldtype: "Link".to_string(),
                label: Some("Customer".to_string()),
                reqd: Some(true),
                unique: Some(false),
                options: Some("Customer".to_string()),
                default: None,
                allow_on_submit: Some(false),
            },
            DocField {
                fieldname: "posting_date".to_string(),
                fieldtype: "Date".to_string(),
                label: Some("Posting Date".to_string()),
                reqd: Some(true),
                unique: Some(false),
                options: None,
                default: None,
                allow_on_submit: Some(false),
            },
            DocField {
                fieldname: "total_amount".to_string(),
                fieldtype: "Decimal".to_string(),
                label: Some("Total Amount".to_string()),
                reqd: Some(false),
                unique: Some(false),
                options: None,
                default: None,
                allow_on_submit: Some(true),
            },
            DocField {
                fieldname: "invoice_number".to_string(),
                fieldtype: "Data".to_string(),
                label: Some("Invoice Number".to_string()),
                reqd: Some(true),
                unique: Some(true),
                options: None,
                default: None,
                allow_on_submit: Some(false),
            },
            DocField {
                fieldname: "items".to_string(),
                fieldtype: "Table".to_string(),
                label: Some("Items".to_string()),
                reqd: Some(false),
                unique: Some(false),
                options: Some("SalesInvoiceItem".to_string()),
                default: None,
                allow_on_submit: Some(false),
            },
        ],
    };

    let ddl = compile_schema_to_surrealql(&schema);

    // Assert contains DEFINE TABLE
    assert!(ddl.contains(&"DEFINE TABLE SalesInvoice SCHEMAFULL;".to_string()));

    // Assert contains DEFINE FIELD for required Link type
    assert!(ddl.contains(&"DEFINE FIELD customer ON SalesInvoice TYPE record ASSERT $value != NONE;".to_string()));

    // Assert contains DEFINE FIELD for required Date type
    assert!(ddl.contains(&"DEFINE FIELD posting_date ON SalesInvoice TYPE datetime ASSERT $value != NONE;".to_string()));

    // Assert contains DEFINE FIELD for Decimal type mapped to SurrealDB decimal
    assert!(ddl.contains(&"DEFINE FIELD total_amount ON SalesInvoice TYPE decimal;".to_string()));

    // Assert contains DEFINE FIELD for items child table mapped to array
    assert!(ddl.contains(&"DEFINE FIELD items ON SalesInvoice TYPE array;".to_string()));

    // Assert contains unique index for invoice_number
    assert!(ddl.contains(&"DEFINE INDEX invoice_number_unique ON SalesInvoice COLUMNS invoice_number UNIQUE;".to_string()));
}
