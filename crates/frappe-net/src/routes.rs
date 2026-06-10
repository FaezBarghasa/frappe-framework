use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;
use serde_json::Value;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;
use crate::middleware::tenant::TenantContext;
use crate::middleware::auth::UserClaims;
use frappe_meta::registry::SchemaRegistry;
use frappe_meta::migration::SchemaManager;

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
            .route("/press/provision", web::post().to(provision_tenant))
    );
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
