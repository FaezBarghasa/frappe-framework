use actix_web::{web, App, HttpServer, HttpResponse};
use surrealdb::engine::remote::ws::Client;
use surrealdb::engine::remote::ws::Ws;
use surrealdb::Surreal;
use std::sync::Arc;
use frappe_net::routes;
use frappe_net::middleware::tenant::TenantMiddleware;

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
    db.signin(surrealdb::opt::auth::Root {
        username: "root",
        password: "root",
    }).await.unwrap_or_else(|e| {
        log::error!("Failed to authenticate with SurrealDB: {}", e);
    });

    let db_data = web::Data::new(db);

    log::info!("Actix-Web HTTP server running at http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(db_data.clone())
            .wrap(TenantMiddleware)
            // .wrap(AuthMiddleware) -> Omitted here for simplicity, apply per route in configure_routes
            .configure(routes::configure_routes)
            .route("/health", web::get().to(|| async { HttpResponse::Ok().body("OK") }))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
