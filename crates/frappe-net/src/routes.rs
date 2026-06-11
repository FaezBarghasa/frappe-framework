use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;
use serde_json::Value;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;
use crate::middleware::tenant::TenantContext;
use crate::middleware::auth::UserClaims;
use frappe_meta::registry::SchemaRegistry;
use frappe_meta::migration::SchemaManager;
use crate::sse::sse_handler;
use futures_util::StreamExt;

pub type Db = Surreal<Client>;

#[derive(Deserialize)]
pub struct ProvisionRequest {
    pub tenant_id: String,
    pub db_user: String,
    pub db_pass: String,
}

/// Configures route endpoints for Actix-Web.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2")
            .route("/document/{doctype}", web::post().to(create_document))
            .route("/document/{doctype}/{id}", web::get().to(get_document))
            .route("/document/{doctype}/{id}", web::put().to(update_document))
            .route("/document/{doctype}/{id}", web::delete().to(delete_document))
            .route("/document/{doctype}/{id}/submit", web::post().to(submit_document))
            .route("/document/{doctype}/{id}/cancel", web::post().to(cancel_document))
            .route("/events", web::get().to(sse_handler))
            .route("/press/provision", web::post().to(provision_tenant))
    );
    cfg.route("/ws", web::get().to(ws_handler));
}

/// Create a new document in SurrealDB.
///
/// Algorithmic Complexity: $O(1)$ database insertion.
async fn create_document(
    db: web::Data<Db>,
    tenant: web::ReqData<TenantContext>,
    path: web::Path<String>,
    body: web::Json<Value>,
) -> impl Responder {
    let doctype = path.into_inner();
    let tenant_id = &tenant.tenant_id;

    // Use specific SurrealDB namespace matching tenant_id and database name
    let db = db.into_inner();
    let query_db = match db.use_ns(tenant_id).use_db("frappe").await {
        Ok(_) => db,
        Err(e) => {
            log::error!("Database namespace switch failed: {:?}", e);
            return HttpResponse::InternalServerError().body("Database connection failure");
        }
    };

    let unique_id = uuid::Uuid::new_v4().to_string();
    
    match query_db.create::<Option<Value>>((doctype, unique_id)).content(body.into_inner()).await {
        Ok(Some(doc)) => HttpResponse::Created().json(doc),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            log::error!("Insert document failed: {:?}", e);
            HttpResponse::InternalServerError().body(format!("DB error: {:?}", e))
        }
    }
}

/// Read document from SurrealDB.
async fn get_document(
    db: web::Data<Db>,
    tenant: web::ReqData<TenantContext>,
    path: web::Path<(String, String)>,
) -> impl Responder {
    let (doctype, id) = path.into_inner();
    let tenant_id = &tenant.tenant_id;

    let db = db.into_inner();
    if let Err(e) = db.use_ns(tenant_id).use_db("frappe").await {
        log::error!("Database namespace switch failed: {:?}", e);
        return HttpResponse::InternalServerError().body("Database connection failure");
    }

    match db.select::<Option<Value>>((doctype, id)).await {
        Ok(Some(doc)) => HttpResponse::Ok().json(doc),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            log::error!("Select document failed: {:?}", e);
            HttpResponse::InternalServerError().body(format!("DB error: {:?}", e))
        }
    }
}

/// Update document in SurrealDB.
async fn update_document(
    db: web::Data<Db>,
    tenant: web::ReqData<TenantContext>,
    path: web::Path<(String, String)>,
    body: web::Json<Value>,
) -> impl Responder {
    let (doctype, id) = path.into_inner();
    let tenant_id = &tenant.tenant_id;

    let db = db.into_inner();
    if let Err(e) = db.use_ns(tenant_id).use_db("frappe").await {
        log::error!("Database namespace switch failed: {:?}", e);
        return HttpResponse::InternalServerError().body("Database connection failure");
    }

    match db.update::<Option<Value>>((doctype, id)).content(body.into_inner()).await {
        Ok(Some(doc)) => HttpResponse::Ok().json(doc),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            log::error!("Update document failed: {:?}", e);
            HttpResponse::InternalServerError().body(format!("DB error: {:?}", e))
        }
    }
}

/// Delete document from SurrealDB.
async fn delete_document(
    db: web::Data<Db>,
    tenant: web::ReqData<TenantContext>,
    path: web::Path<(String, String)>,
) -> impl Responder {
    let (doctype, id) = path.into_inner();
    let tenant_id = &tenant.tenant_id;

    let db = db.into_inner();
    if let Err(e) = db.use_ns(tenant_id).use_db("frappe").await {
        log::error!("Database namespace switch failed: {:?}", e);
        return HttpResponse::InternalServerError().body("Database connection failure");
    }

    match db.delete::<Option<Value>>((doctype, id)).await {
        Ok(Some(doc)) => HttpResponse::Ok().json(doc),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            log::error!("Delete document failed: {:?}", e);
            HttpResponse::InternalServerError().body(format!("DB error: {:?}", e))
        }
    }
}

/// Provision a new tenant namespace and database in SurrealDB.
async fn provision_tenant(
    db: web::Data<Db>,
    claims: Option<web::ReqData<UserClaims>>,
    body: web::Json<ProvisionRequest>,
) -> impl Responder {
    // Check permissions
    if let Some(claims) = claims {
        if !claims.roles.contains(&"System Manager".to_string()) && !claims.roles.contains(&"root".to_string()) {
            return HttpResponse::Forbidden().body("Insufficient permissions");
        }
    } else {
        return HttpResponse::Unauthorized().body("Authentication required");
    }

    let req = body.into_inner();
    let tenant_id = req.tenant_id;
    let db = db.into_inner();

    // 1. Provision namespaces using SurrealQL
    let query = format!(
        "DEFINE NAMESPACE {}; \
         USE NS {}; \
         DEFINE DATABASE frappe; \
         USE DB frappe; \
         DEFINE USER {} ON NAMESPACE PASSWORD '{}' ROLES OWNER;",
        tenant_id, tenant_id, req.db_user, req.db_pass
    );

    match db.query(&query).await {
        Ok(res) => {
            if let Err(e) = res.check() {
                return HttpResponse::InternalServerError().body(format!("Failed to execute provisioning DDL: {}", e));
            }
        }
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Provisioning failed: {}", e));
        }
    }

    // Switch to new namespace for DDL execution
    if let Err(e) = db.use_ns(&tenant_id).use_db("frappe").await {
        return HttpResponse::InternalServerError().body(format!("Failed to switch to provisioned database: {}", e));
    }

    // 2. Initialize default schemas
    let registry = SchemaRegistry::new();
    // Assuming "apps" is the default directory for schemas, and ignoring errors during crawl for now to continue provision
    let _ = registry.crawl_and_load_schemas("apps").await;

    let schemas = registry.get_all_schemas();
    let manager = SchemaManager::new(&db);
    for schema in schemas {
        if let Err(e) = manager.sync_schema(&schema).await {
            log::error!("Failed to execute DDL for schema {}: {}", schema.name, e);
            // Non-fatal error for provisioning, but logged
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "message": format!("Tenant {} provisioned successfully", tenant_id)
    }))
}

/// Submit a document in SurrealDB.
async fn submit_document(
    db: web::Data<Db>,
    tenant: web::ReqData<TenantContext>,
    path: web::Path<(String, String)>,
    registry: web::Data<SchemaRegistry>,
) -> impl Responder {
    let (doctype, id) = path.into_inner();
    let tenant_id = &tenant.tenant_id;
    let db = db.into_inner();

    if let Err(e) = db.use_ns(tenant_id).use_db("frappe").await {
        log::error!("Database namespace switch failed: {:?}", e);
        return HttpResponse::InternalServerError().body("Database connection failure");
    }

    let doc: Option<Value> = match db.select((doctype.as_str(), id.as_str())).await {
        Ok(d) => d,
        Err(e) => {
            log::error!("Select document failed: {:?}", e);
            return HttpResponse::InternalServerError().body(format!("DB error: {:?}", e));
        }
    };

    let doc_val = match doc {
        Some(Value::Object(map)) => map,
        _ => return HttpResponse::NotFound().finish(),
    };

    let mut new_doc_val = doc_val.clone();
    new_doc_val.insert("docstatus".to_string(), Value::Number(1.into()));

    let schema = match registry.get_schema(&doctype) {
        Some(s) => s,
        None => {
            return HttpResponse::BadRequest().body(format!("Schema not found for doctype: {}", doctype));
        }
    };

    if let Err(e) = frappe_framework::document::lifecycle::DocLifecycleController::validate_transition(&doc_val, &new_doc_val, &schema) {
        return HttpResponse::BadRequest().body(format!("Validation failed: {:?}", e));
    }

    match db.update::<Option<Value>>((doctype.as_str(), id.as_str())).content(Value::Object(new_doc_val)).await {
        Ok(Some(updated_doc)) => HttpResponse::Ok().json(updated_doc),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            log::error!("Submit document failed: {:?}", e);
            HttpResponse::InternalServerError().body(format!("DB error: {:?}", e))
        }
    }
}

/// Cancel a document in SurrealDB.
async fn cancel_document(
    db: web::Data<Db>,
    tenant: web::ReqData<TenantContext>,
    path: web::Path<(String, String)>,
    registry: web::Data<SchemaRegistry>,
) -> impl Responder {
    let (doctype, id) = path.into_inner();
    let tenant_id = &tenant.tenant_id;
    let db = db.into_inner();

    if let Err(e) = db.use_ns(tenant_id).use_db("frappe").await {
        log::error!("Database namespace switch failed: {:?}", e);
        return HttpResponse::InternalServerError().body("Database connection failure");
    }

    let doc: Option<Value> = match db.select((doctype.as_str(), id.as_str())).await {
        Ok(d) => d,
        Err(e) => {
            log::error!("Select document failed: {:?}", e);
            return HttpResponse::InternalServerError().body(format!("DB error: {:?}", e));
        }
    };

    let doc_val = match doc {
        Some(Value::Object(map)) => map,
        _ => return HttpResponse::NotFound().finish(),
    };

    let mut new_doc_val = doc_val.clone();
    new_doc_val.insert("docstatus".to_string(), Value::Number(2.into()));

    let schema = match registry.get_schema(&doctype) {
        Some(s) => s,
        None => {
            return HttpResponse::BadRequest().body(format!("Schema not found for doctype: {}", doctype));
        }
    };

    if let Err(e) = frappe_framework::document::lifecycle::DocLifecycleController::validate_transition(&doc_val, &new_doc_val, &schema) {
        return HttpResponse::BadRequest().body(format!("Validation failed: {:?}", e));
    }

    match db.update::<Option<Value>>((doctype.as_str(), id.as_str())).content(Value::Object(new_doc_val)).await {
        Ok(Some(updated_doc)) => HttpResponse::Ok().json(updated_doc),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(e) => {
            log::error!("Cancel document failed: {:?}", e);
            HttpResponse::InternalServerError().body(format!("DB error: {:?}", e))
        }
    }
}

/// WebSocket route handler
async fn ws_handler(
    req: actix_web::HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let (res, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    actix_web::rt::spawn(async move {
        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                actix_ws::Message::Text(text) => {
                    let _ = session.text(text).await;
                }
                actix_ws::Message::Ping(bytes) => {
                    let _ = session.pong(&bytes).await;
                }
                actix_ws::Message::Close(reason) => {
                    let _ = session.close(reason).await;
                    break;
                }
                _ => {}
            }
        }
    });
    Ok(res)
}
