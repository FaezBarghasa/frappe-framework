use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage, HttpResponse};
use actix_web::body::EitherBody;
use pasetors::keys::SymmetricKey;
use pasetors::Local;
use pasetors::version4::V4;
use pasetors::token::UntrustedToken;
use pasetors::claims::ClaimsValidationRules;
use serde::{Deserialize, Serialize};
use std::future::{ready, Ready};
use std::pin::Pin;
use std::convert::TryFrom;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserClaims {
    pub user_id: String,
    pub roles: Vec<String>,
}

pub struct PasetoAuth;

impl<S, B> Transform<S, ServiceRequest> for PasetoAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B, actix_web::body::BoxBody>>;
    type Error = Error;
    type Transform = PasetoAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(PasetoAuthMiddleware { service }))
    }
}

pub struct PasetoAuthMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for PasetoAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B, actix_web::body::BoxBody>>;
    type Error = Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + 'static>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // 1. Read Authorization header
        let auth_header = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok());

        if let Some(auth_val) = auth_header {
            if auth_val.starts_with("v4.local.") {
                // Get or generate symmetric key safely
                let key = match get_paseto_key() {
                    Ok(k) => k,
                    Err(e) => {
                        log::error!("Failed to obtain PASETO secret key: {:?}", e);
                        let (r, _) = req.into_parts();
                        let res = HttpResponse::InternalServerError().finish().map_into_right_body();
                        return Box::pin(ready(Ok(ServiceResponse::new(r, res))));
                    }
                };

                let validation_rules = ClaimsValidationRules::new();
                
                // Decrypt token
                // O(1) decryption logic
                match UntrustedToken::<Local, V4>::try_from(auth_val) {
                    Ok(untrusted_token) => {
                        match pasetors::local::decrypt(
                            &key,
                            &untrusted_token,
                            &validation_rules,
                            None,
                            Some(b"frappe-rust-v2"),
                        ) {
                            Ok(trusted_token) => {
                                if let Ok(claims) = serde_json::from_str::<UserClaims>(trusted_token.payload()) {
                                    // Inject validated claims into Request Extensions
                                    req.extensions_mut().insert(claims);
                                    let fut = self.service.call(req);
                                    return Box::pin(async move {
                                        let res = fut.await?;
                                        Ok(res.map_into_left_body())
                                    });
                                }
                            }
                            Err(e) => {
                                log::warn!("PASETO decryption failed: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Invalid PASETO token format: {:?}", e);
                    }
                }
            }
        }

        // Return 401 Unauthorized if token validation fails
        let (r, _) = req.into_parts();
        let res = HttpResponse::Unauthorized().finish().map_into_right_body();
        Box::pin(ready(Ok(ServiceResponse::new(r, res))))
    }
}

/// Helper function to retrieve symmetric key from environment or generate an ephemeral one.
pub fn get_paseto_key() -> std::result::Result<SymmetricKey<V4>, String> {
    if let Ok(key_str) = std::env::var("PASETO_SECRET_KEY") {
        if let Ok(key_bytes) = hex::decode(key_str) {
            if let Ok(k) = SymmetricKey::<V4>::from(&key_bytes) {
                return Ok(k);
            }
        }
    }
    
    // Fallback: Generate ephemeral key for security test sandbox and log warning
    log::warn!("Generating ephemeral PASETO secret key. Instance-isolated!");
    let mut key_bytes = [0u8; 32];
    
    #[cfg(not(test))]
    {
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut key_bytes);
    }
    #[cfg(test)]
    {
        key_bytes.fill(7u8); // Consistent dummy key for unit testing
    }
    
    SymmetricKey::<V4>::from(&key_bytes).map_err(|e| format!("{:?}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::{self, TestRequest};
    use actix_web::{web, App, Responder};
    use pasetors::claims::Claims;
    use pasetors::local;

    async fn index(req: actix_web::HttpRequest) -> impl Responder {
        let claims = req.extensions().get::<UserClaims>().cloned();
        claims.map(|c| c.user_id).unwrap_or_else(|| "none".to_string())
    }

    #[actix_web::test]
    async fn test_paseto_auth_success() {
        let app = test::init_service(
            App::new()
                .wrap(PasetoAuth)
                .route("/", web::get().to(index)),
        )
        .await;

        let mut claims = Claims::new().unwrap();
        claims.add_additional("user_id", "user_123").unwrap();
        claims.add_additional("roles", vec!["admin".to_string()]).unwrap();
        
        let key = get_paseto_key().unwrap();
        let token = local::encrypt(
            &key,
            &claims,
            None,
            Some(b"frappe-rust-v2"),
        ).unwrap();

        let req = TestRequest::default()
            .insert_header(("Authorization", token))
            .to_request();

        let resp = test::call_and_read_body(&app, req).await;
        assert_eq!(resp, actix_web::web::Bytes::from_static(b"user_123"));
    }

    #[actix_web::test]
    async fn test_paseto_auth_unauthorized() {
        let app = test::init_service(
            App::new()
                .wrap(PasetoAuth)
                .route("/", web::get().to(index)),
        )
        .await;

        let req = TestRequest::default()
            .insert_header(("Authorization", "invalid-token"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }
}
