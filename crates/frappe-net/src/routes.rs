use actix_web::{web, HttpResponse, Responder};
use serde_json::Value;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;
use crate::middleware::tenant::TenantContext;

pub type Db = Surreal<Client>;

/// Configures route endpoints for Actix-Web.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2")
            .route("/document/{doctype}", web::post().to(create_document))
            .route("/document/{doctype}/{id}", web::get().to(get_document))
            .route("/document/{doctype}/{id}", web::put().to(update_document))
            .route("/document/{doctype}/{id}", web::delete().to(delete_document))
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
