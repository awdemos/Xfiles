use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Simple token-bucket rate limiter keyed by client IP.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    buckets: Arc<DashMap<String, TokenBucket>>,
    max_requests: u64,
    window_secs: u64,
}

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: u64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(max_requests: u64, window_secs: u64) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            max_requests,
            window_secs,
        }
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(self.window_secs);

        let mut entry = self.buckets.entry(key.to_string()).or_insert_with(|| TokenBucket {
            tokens: self.max_requests,
            last_refill: now,
        });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(entry.last_refill);
        let refill = (elapsed.as_secs_f64() / window.as_secs_f64() * self.max_requests as f64) as u64;
        if refill > 0 {
            entry.tokens = (entry.tokens + refill).min(self.max_requests);
            entry.last_refill = now;
        }

        if entry.tokens > 0 {
            entry.tokens -= 1;
            true
        } else {
            false
        }
    }
}

/// Tower middleware using the rate limiter.
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    // Extract client IP or fallback to path
    let key = request
        .extensions()
        .get::<SocketAddr>()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| request.uri().path().to_string());

    if limiter.check(&key) {
        next.run(request).await
    } else {
        (
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded. Try again later.",
        )
            .into_response()
    }
}
