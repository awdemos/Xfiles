use xfiles::message::Message;
use xfiles::plumber::Plumber;

#[tokio::test]
async fn test_plumber_routes_by_type() {
    let plumber = Plumber::new();
    plumber
        .add_rule("ai", "type:llm_request", "/ai/inference", 100, None)
        .unwrap();

    let msg = Message::new("test", "/prompt", "llm_request");
    let dests = plumber.route(&msg);

    assert_eq!(dests, vec!["/ai/inference"]);
}

#[tokio::test]
async fn test_plumber_fallback_to_log() {
    let plumber = Plumber::new();
    plumber
        .add_rule("ai", "type:llm_request", "/ai/inference", 100, None)
        .unwrap();

    let msg = Message::new("test", "/prompt", "unknown_type");
    let dests = plumber.route(&msg);

    assert_eq!(dests, vec!["/proc/log"]);
}

#[tokio::test]
async fn test_plumber_priority_order() {
    let plumber = Plumber::new();
    plumber
        .add_rule("low", "type:test", "/low", 10, None)
        .unwrap();
    plumber
        .add_rule("high", "type:test", "/high", 100, None)
        .unwrap();

    let rules = plumber.list_rules();
    assert_eq!(rules[0].2, 100); // highest priority first
    assert_eq!(rules[1].2, 10);
}
