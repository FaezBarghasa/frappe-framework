use actix_web::test::{self, TestRequest};
use actix_web::{web, App, HttpMessage, Responder};
use frappe_net::middleware::tenant::{TenantContext, TenantResolver};

async fn test_handler(req: actix_web::HttpRequest) -> impl Responder {
    let tenant = req.extensions().get::<TenantContext>().cloned();
    tenant.map(|t| t.tenant_id).unwrap_or_else(|| "none".to_string())
}

#[actix_web::test]
async fn test_tenant_isolation_parallel_requests() {
    let app = test::init_service(
        App::new()
            .wrap(TenantResolver)
            .route("/", web::get().to(test_handler)),
    )
    .await;

    // Send requests to tenant1 and tenant2 in parallel
    let req1 = TestRequest::default()
        .insert_header(("Host", "tenant1.localhost"))
        .to_request();
    let req2 = TestRequest::default()
        .insert_header(("Host", "tenant2.localhost"))
        .to_request();

    // Call service concurrently
    let fut1 = test::call_and_read_body(&app, req1);
    let fut2 = test::call_and_read_body(&app, req2);
    let (res1, res2) = futures_util::join!(fut1, fut2);

    assert_eq!(res1, actix_web::web::Bytes::from_static(b"tenant1"));
    assert_eq!(res2, actix_web::web::Bytes::from_static(b"tenant2"));
}
