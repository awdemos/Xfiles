use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use std::sync::Arc;
use tower::ServiceExt;

async fn dummy_handler() -> &'static str {
    "ok"
}

#[tokio::test]
async fn test_auth_allows_public_routes() {
    let auth_config = Arc::new(xfiles::auth::AuthConfig {
        api_key: Some("secret".into()),
        agent_token: None,
    });

    let app = Router::new()
        .route("/health", get(dummy_handler))
        .layer(axum::middleware::from_fn_with_state(
            auth_config.clone(),
            xfiles::auth::api_key_middleware,
        ));

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_blocks_missing_key() {
    let auth_config = Arc::new(xfiles::auth::AuthConfig {
        api_key: Some("secret".into()),
        agent_token: None,
    });

    let app = Router::new()
        .route("/protected", get(dummy_handler))
        .layer(axum::middleware::from_fn_with_state(
            auth_config.clone(),
            xfiles::auth::api_key_middleware,
        ));

    let response = app
        .oneshot(Request::builder().uri("/protected").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_allows_valid_key() {
    let auth_config = Arc::new(xfiles::auth::AuthConfig {
        api_key: Some("secret".into()),
        agent_token: None,
    });

    let app = Router::new()
        .route("/protected", get(dummy_handler))
        .layer(axum::middleware::from_fn_with_state(
            auth_config.clone(),
            xfiles::auth::api_key_middleware,
        ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header("Authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
