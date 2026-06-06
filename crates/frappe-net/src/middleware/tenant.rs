use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage};
use std::future::{ready, Ready};
use std::pin::Pin;

#[derive(Clone, Debug)]
pub struct TenantContext {
    pub tenant_id: String,
}

pub struct TenantResolver;

impl<S, B> Transform<S, ServiceRequest> for TenantResolver
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = TenantResolverMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TenantResolverMiddleware { service }))
    }
}

pub struct TenantResolverMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for TenantResolverMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + 'static>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Extract host header
        let host = req
            .headers()
            .get("Host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("default_site");

        // Sanitize Host string: Replace non-alphanumeric characters with underscores
        // O(N) complexity where N is host length
        let sanitized_host: String = host
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();

        let tenant_context = TenantContext {
            tenant_id: sanitized_host,
        };

        // Inject in O(1) complexity
        req.extensions_mut().insert(tenant_context);

        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            Ok(res)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::{self, TestRequest};
    use actix_web::{web, App, Responder};

    async fn index(req: actix_web::HttpRequest) -> impl Responder {
        let tenant = req.extensions().get::<TenantContext>().cloned();
        tenant.map(|t| t.tenant_id).unwrap_or_else(|| "none".to_string())
    }

    #[actix_web::test]
    async fn test_tenant_resolver_middleware() {
        let app = test::init_service(
            App::new()
                .wrap(TenantResolver)
                .route("/", web::get().to(index)),
        )
        .await;

        let req = TestRequest::default()
            .insert_header(("Host", "site1.local"))
            .to_request();

        let resp = test::call_and_read_body(&app, req).await;
        assert_eq!(resp, actix_web::web::Bytes::from_static(b"site1_local"));
    }
}
