use xfiles::message::Message;
use xfiles::store::Store;

fn temp_db_url(name: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "xfiles-store-test-{}-{}.db",
        name,
        uuid::Uuid::new_v4()
    ));
    format!("sqlite:{}", path.display())
}

fn cleanup_db(url: &str) {
    let path = url.strip_prefix("sqlite:").unwrap();
    for suffix in ["", "-wal", "-shm"] {
        std::fs::remove_file(format!("{}{}", path, suffix)).ok();
    }
}

#[tokio::test]
async fn store_creates_missing_database_file() {
    let url = temp_db_url("create");
    let db_path = url.strip_prefix("sqlite:").unwrap().to_string();
    assert!(!std::path::Path::new(&db_path).exists());

    let store = Store::new(&url).await.unwrap();
    assert!(
        std::path::Path::new(&db_path).exists(),
        "store should create the database file on fresh install"
    );

    let msg = Message::new("tester", "/net/x", "test_msg").with_data(serde_json::json!({"k": 1}));
    let cid = msg.conversation_id;
    store.insert_message(&msg).await.unwrap();

    let msgs = store.get_messages_by_conversation(cid, 10).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].id, msg.id);
    assert_eq!(msgs[0].data, serde_json::json!({"k": 1}));

    drop(store);
    cleanup_db(&url);
}

#[tokio::test]
async fn store_corrupt_message_data_falls_back_to_null() {
    let url = temp_db_url("corrupt");
    let store = Store::new(&url).await.unwrap();

    let msg = Message::new("tester", "/net/x", "test_msg").with_data(serde_json::json!({"k": 1}));
    let cid = msg.conversation_id;
    store.insert_message(&msg).await.unwrap();

    let raw = sqlx::SqlitePool::connect(&url).await.unwrap();
    sqlx::query("UPDATE messages SET data = '{not valid json' WHERE id = ?1")
        .bind(msg.id.to_string())
        .execute(&raw)
        .await
        .unwrap();
    raw.close().await;

    let msgs = store.get_messages_by_conversation(cid, 10).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].id, msg.id);
    assert_eq!(msgs[0].data, serde_json::Value::Null);

    drop(store);
    cleanup_db(&url);
}
