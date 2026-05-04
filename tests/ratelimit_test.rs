use xfiles::ratelimit::RateLimiter;

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
