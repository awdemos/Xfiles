/// Xfiles WebSocket Client Example
///
/// Run with:
///   cargo run --example ws_client -- --agent-id my-laptop
///
/// This demonstrates connecting to an Xfiles hub, registering capabilities,
/// sending messages, and reading virtual files.

use clap::Parser;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

#[derive(Parser, Debug)]
#[command(name = "ws_client")]
struct Args {
    #[arg(short, long, default_value = "ws://localhost:9999/ws/example-agent")]
    url: String,
    #[arg(short, long, default_value = "example-agent")]
    agent_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!("Connecting to Xfiles hub at {} ...", args.url);
    let (ws_stream, _) = connect_async(&args.url).await?;
    let (mut write, mut read) = ws_stream.split();

    // 1. Send Hello with capability manifest
    let hello = serde_json::json!({
        "op": "hello",
        "manifest": {
            "agent_id": args.agent_id,
            "hostname": std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".into()),
            "capabilities": [
                {
                    "name": "chat",
                    "version": "1.0",
                    "paths": ["/send", "/recv"]
                }
            ]
        }
    });
    write.send(WsMessage::Text(hello.to_string())).await?;
    println!(">>> Sent: {}", hello);

    // 2. Spawn read task
    let read_handle = tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            match msg {
                WsMessage::Text(text) => {
                    println!("<<< Received: {}", text);
                }
                WsMessage::Close(_) => {
                    println!("<<< Connection closed");
                    break;
                }
                _ => {}
            }
        }
    });

    // 3. Send periodic heartbeats
    let mut heartbeat_interval = tokio::time::interval(std::time::Duration::from_secs(10));

    // 4. Send a test message
    let test_msg = serde_json::json!({
        "op": "message",
        "id": uuid::Uuid::new_v4().to_string(),
        "conversation_id": uuid::Uuid::new_v4().to_string(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "sender": args.agent_id,
        "sender_ns": format!("/net/{}", args.agent_id),
        "path": "/ai/inference",
        "type": "llm_request",
        "data": {
            "prompt": "Explain quantum computing in one sentence."
        },
        "headers": {}
    });
    write.send(WsMessage::Text(test_msg.to_string())).await?;
    println!(">>> Sent: {}", test_msg);

    // 5. Read a virtual file
    let fs_read = serde_json::json!({
        "op": "fs_read",
        "path": format!("/net/{}/ctl/status", args.agent_id)
    });
    write.send(WsMessage::Text(fs_read.to_string())).await?;
    println!(">>> Sent: {}", fs_read);

    // Keep alive with heartbeats
    loop {
        heartbeat_interval.tick().await;
        let hb = serde_json::json!({ "op": "heartbeat" });
        if write.send(WsMessage::Text(hb.to_string())).await.is_err() {
            break;
        }
        println!(">>> Sent heartbeat");
    }

    let _ = read_handle.await;
    Ok(())
}
