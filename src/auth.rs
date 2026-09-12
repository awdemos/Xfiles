use axum::{
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// Compare two tokens without early-exiting on the first differing byte.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Authentication configuration.
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    /// API key required for proxy and admin endpoints.
    pub api_key: Option<String>,
    /// Optional token required for WebSocket agent connections.
    pub agent_token: Option<String>,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        Self {
            api_key: std::env::var("XFILES_API_KEY").ok(),
            agent_token: std::env::var("XFILES_AGENT_TOKEN").ok(),
        }
    }
}

/// Tower middleware that enforces bearer-token auth on protected routes.
pub async fn api_key_middleware(
    State(config): State<Arc<AuthConfig>>,
    request: Request,
    next: Next,
) -> Response {
    // CORS preflight requests carry no credentials
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }

    // Public routes
    let path = request.uri().path();
    if path == "/health" || path == "/metrics" || path.starts_with("/ws/") {
        return next.run(request).await;
    }

    if let Some(ref key) = config.api_key {
        let auth_header = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let valid = match auth_header {
            Some(header) if header.starts_with("Bearer ") => constant_time_eq(&header[7..], key),
            _ => false,
        };

        if !valid {
            return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }
    }

    next.run(request).await
}

/// WebSocket-specific auth check (agent token).
pub fn check_agent_token(config: &AuthConfig, headers: &axum::http::HeaderMap) -> bool {
    if let Some(ref token) = config.agent_token {
        let auth_header = headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        match auth_header {
            Some(header) if header.starts_with("Bearer ") => constant_time_eq(&header[7..], token),
            _ => false,
        }
    } else {
        true
    }
}
