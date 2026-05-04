use xfiles::circuit::{CircuitBreaker, CircuitState};

#[tokio::test]
async fn test_circuit_starts_closed() {
    let cb = CircuitBreaker::new(3, 1, 1);
    assert!(cb.allow("ep-1"));
    assert_eq!(cb.get_state("ep-1"), CircuitState::Closed);
}

#[tokio::test]
async fn test_circuit_opens_after_failures() {
    let cb = CircuitBreaker::new(3, 1, 1);
    cb.record_failure("ep-1");
    cb.record_failure("ep-1");
    assert!(cb.allow("ep-1")); // still closed at 2 failures
    cb.record_failure("ep-1");
    assert_eq!(cb.get_state("ep-1"), CircuitState::Open);
    assert!(!cb.allow("ep-1"));
}

#[tokio::test]
async fn test_circuit_half_open_then_closes() {
    let cb = CircuitBreaker::new(2, 1, 1);
    cb.record_failure("ep-1");
    cb.record_failure("ep-1");
    assert_eq!(cb.get_state("ep-1"), CircuitState::Open);

    // Wait for recovery timeout
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(cb.allow("ep-1")); // half-open
    assert_eq!(cb.get_state("ep-1"), CircuitState::HalfOpen);

    cb.record_success("ep-1");
    assert_eq!(cb.get_state("ep-1"), CircuitState::Closed);
    assert!(cb.allow("ep-1"));
}

#[tokio::test]
async fn test_circuit_half_open_then_reopens() {
    let cb = CircuitBreaker::new(2, 1, 1);
    cb.record_failure("ep-1");
    cb.record_failure("ep-1");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(cb.allow("ep-1"));
    cb.record_failure("ep-1");
    assert_eq!(cb.get_state("ep-1"), CircuitState::Open);
    assert!(!cb.allow("ep-1"));
}
