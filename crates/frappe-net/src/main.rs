use actix_web::{web, App, HttpServer, HttpResponse};
use surrealdb::engine::remote::ws::Ws;
use surrealdb::Surreal;
use frappe_net::routes;
use frappe_net::middleware::tenant::TenantResolver;
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    log::info!("Starting Caffeine-Rust Unified API Server...");

    // Connect to local SurrealDB instance
    let db = Surreal::new::<Ws>("127.0.0.1:8000").await.unwrap_or_else(|e| {
        log::error!("Failed to connect to SurrealDB: {}", e);
        std::process::exit(1);
    });
    
    // Log in as root (for provisioning / schema purposes)
    if let Err(e) = db.signin(surrealdb::opt::auth::Root {
        username: "root".to_string(),
        password: "root".to_string(),
    }).await {
        log::error!("Failed to authenticate with SurrealDB: {}", e);
    }

    // Initialize SchemaRegistry and ScriptSandbox
    let registry = frappe_meta::registry::SchemaRegistry::new();
    let _ = registry.crawl_and_load_schemas("apps").await;
    
    let db_client = Arc::new(frappe_meta::db::DatabaseClient::new(db.clone()));
    let sandbox = frappe_framework::document::scripting::ScriptSandbox::new(db_client);
    
    let broadcaster = frappe_net::sse::EventBroadcaster::new();

    let db_data = web::Data::new(db);
    let registry_data = web::Data::new(registry);
    let sandbox_data = web::Data::new(sandbox);
    let broadcaster_data = web::Data::new(broadcaster);

    // Spawn HTTP/3 (QUIC) server
    let h3_addr = "127.0.0.1:8443".parse().unwrap();
    let cert_path = std::env::var("TLS_CERT_PATH").unwrap_or_else(|_| "certs/cert.pem".to_string());
    let key_path = std::env::var("TLS_KEY_PATH").unwrap_or_else(|_| "certs/key.pem".to_string());

    let h3_server = frappe_net::h3_server::H3Server::new(h3_addr, cert_path, key_path);
    tokio::spawn(async move {
        log::info!("Starting HTTP/3 Server on UDP 127.0.0.1:8443...");
        if let Err(e) = h3_server.run().await {
            log::error!("HTTP/3 server error: {:?}", e);
        }
    });

    log::info!("Actix-Web HTTP server running at http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(db_data.clone())
            .app_data(registry_data.clone())
            .app_data(sandbox_data.clone())
            .app_data(broadcaster_data.clone())
            .wrap(TenantResolver)
            // .wrap(AuthMiddleware) -> Omitted here for simplicity, apply per route in configure_routes
            .configure(routes::configure_routes)
            .route("/health", web::get().to(|| async { HttpResponse::Ok().body("OK") }))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
