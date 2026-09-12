use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;
use xfiles::ratelimit::RateLimiter;

async fn limited_handler() -> &'static str {
    "ok"
}

#[tokio::test]
async fn test_rate_limiter_allows_under_limit() {
    let limiter = RateLimiter::new(3, 60);
    assert!(limiter.check("client-a"));
    assert!(limiter.check("client-a"));
    assert!(limiter.check("client-a"));
}

#[tokio::test]
async fn test_rate_limiter_blocks_over_limit() {
    let limiter = RateLimiter::new(2, 60);
    assert!(limiter.check("client-b"));
    assert!(limiter.check("client-b"));
    assert!(!limiter.check("client-b"));
}

#[tokio::test]
async fn test_rate_limiter_isolated_per_key() {
    let limiter = RateLimiter::new(1, 60);
    assert!(limiter.check("client-c"));
    assert!(!limiter.check("client-c"));
    assert!(limiter.check("client-d"));
}

#[tokio::test]
async fn test_rate_limiter_zero_config_clamped() {
    let limiter = RateLimiter::new(0, 0);
    assert!(limiter.check("client-e"));
    assert!(!limiter.check("client-e"));
    assert!(limiter.check("client-f"));
}

#[tokio::test]
async fn test_rate_limit_middleware_keys_by_connect_info() {
    let limiter = Arc::new(RateLimiter::new(1, 60));

    let app = Router::new().route("/limited", get(limited_handler)).layer(
        axum::middleware::from_fn_with_state(limiter, xfiles::ratelimit::rate_limit_middleware),
    );

    let addr_a: SocketAddr = "10.0.0.1:4001".parse().unwrap();
    let addr_b: SocketAddr = "10.0.0.2:4002".parse().unwrap();

    let mut request = Request::builder()
        .uri("/limited")
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(addr_a));
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mut request = Request::builder()
        .uri("/limited")
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(addr_a));
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let mut request = Request::builder()
        .uri("/limited")
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(addr_b));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
