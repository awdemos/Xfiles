use crate::message::{FeedbackEvent, Message};
use crate::quantum::state::EndpointState;
use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::collections::HashMap;

/// SQLite-backed persistence layer for Xfiles.
#[derive(Debug, Clone)]
pub struct Store {
    pool: Pool<Sqlite>,
}

impl Store {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        Self::run_migrations(&pool).await?;

        Ok(Self { pool })
    }

    async fn run_migrations(pool: &Pool<Sqlite>) -> anyhow::Result<()> {
        sqlx::migrate!("./migrations").run(pool).await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Messages
    // ------------------------------------------------------------------

    pub async fn insert_message(&self, msg: &Message) -> anyhow::Result<()> {
        let headers = serde_json::to_string(&msg.headers).unwrap_or_default();
        let quantum = msg.quantum.as_ref().map(|q| serde_json::to_string(q).unwrap_or_default());

        sqlx::query(
            r#"
            INSERT INTO messages (id, parent_id, conversation_id, timestamp, sender, sender_ns, path, msg_type, data, headers, quantum)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#
        )
        .bind(msg.id.to_string())
        .bind(msg.parent_id.map(|id| id.to_string()))
        .bind(msg.conversation_id.to_string())
        .bind(msg.timestamp.to_rfc3339())
        .bind(&msg.sender)
        .bind(&msg.sender_ns)
        .bind(&msg.path)
        .bind(&msg.msg_type)
        .bind(msg.data.to_string())
        .bind(headers)
        .bind(quantum)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_messages_by_conversation(
        &self,
        conversation_id: uuid::Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT id, parent_id, conversation_id, timestamp, sender, sender_ns, path, msg_type, data, headers, quantum
            FROM messages
            WHERE conversation_id = ?1
            ORDER BY timestamp DESC
            LIMIT ?2
            "#
        )
        .bind(conversation_id.to_string())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    // ------------------------------------------------------------------
    // Quantum State
    // ------------------------------------------------------------------

    pub async fn load_quantum_state(
        &self,
        conversation_id: uuid::Uuid,
    ) -> anyhow::Result<HashMap<String, EndpointState>> {
        let rows = sqlx::query_as::<_, QuantumStateRow>(
            r#"
            SELECT endpoint_id, real_amplitude, imag_amplitude, pulls, total_reward
            FROM quantum_state
            WHERE conversation_id = ?1
            "#
        )
        .bind(conversation_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut map = HashMap::new();
        for row in rows {
            map.insert(
                row.endpoint_id,
                EndpointState {
                    amplitude: crate::quantum::state::Amplitude::new(row.real_amplitude, row.imag_amplitude),
                    pulls: row.pulls as u64,
                    total_reward: row.total_reward,
                    last_updated: Utc::now(),
                },
            );
        }
        Ok(map)
    }

    pub async fn save_quantum_state(
        &self,
        conversation_id: uuid::Uuid,
        endpoint_id: &str,
        state: &EndpointState,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO quantum_state (conversation_id, endpoint_id, real_amplitude, imag_amplitude, pulls, total_reward, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(conversation_id, endpoint_id) DO UPDATE SET
                real_amplitude = excluded.real_amplitude,
                imag_amplitude = excluded.imag_amplitude,
                pulls = excluded.pulls,
                total_reward = excluded.total_reward,
                updated_at = excluded.updated_at
            "#
        )
        .bind(conversation_id.to_string())
        .bind(endpoint_id)
        .bind(state.amplitude.real)
        .bind(state.amplitude.imag)
        .bind(state.pulls as i64)
        .bind(state.total_reward)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ------------------------------------------------------------------
    // Feedback
    // ------------------------------------------------------------------

    pub async fn insert_feedback(&self, event: &FeedbackEvent) -> anyhow::Result<()> {
        let metadata = event.metadata.as_ref().map(|m| m.to_string());

        sqlx::query(
            r#"
            INSERT INTO feedback_events (message_id, endpoint_id, success, latency_ms, quality_score, error_kind, metadata)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#
        )
        .bind(event.message_id.to_string())
        .bind(&event.endpoint_id)
        .bind(event.success)
        .bind(event.latency_ms as i64)
        .bind(event.quality_score)
        .bind(event.error_kind.clone())
        .bind(metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_feedback_stats(&self, endpoint_id: &str) -> anyhow::Result<(u64, f64)> {
        let row: (i64, f64) = sqlx::query_as(
            r#"
            SELECT COUNT(*), AVG(latency_ms)
            FROM feedback_events
            WHERE endpoint_id = ?1 AND success = 1
            "#
        )
        .bind(endpoint_id)
        .fetch_one(&self.pool)
        .await?;

        Ok((row.0 as u64, row.1))
    }

    // ------------------------------------------------------------------
    // Delivery Tracking
    // ------------------------------------------------------------------

    pub async fn update_delivery_status(
        &self,
        message_id: uuid::Uuid,
        status: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE messages
            SET delivery_status = ?1, delivered_at = ?2
            WHERE id = ?3
            "#
        )
        .bind(status)
        .bind(Utc::now().to_rfc3339())
        .bind(message_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_delivery_attempt(
        &self,
        message_id: uuid::Uuid,
        endpoint_id: Option<&str>,
        agent_id: Option<&str>,
        status: &str,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO delivery_attempts (message_id, endpoint_id, agent_id, status, error)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#
        )
        .bind(message_id.to_string())
        .bind(endpoint_id)
        .bind(agent_id)
        .bind(status)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Conversations
    // ------------------------------------------------------------------

    pub async fn list_conversations(&self, limit: i64) -> anyhow::Result<Vec<(uuid::Uuid, i64)>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT conversation_id, COUNT(*) as message_count
            FROM messages
            GROUP BY conversation_id
            ORDER BY MAX(timestamp) DESC
            LIMIT ?1
            "#
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|(cid, count)| Ok((cid.parse()?, count)))
            .collect()
    }

    // ------------------------------------------------------------------
    // Endpoint History
    // ------------------------------------------------------------------

    pub async fn log_endpoint_event(
        &self,
        endpoint_id: &str,
        endpoint_name: &str,
        url: &str,
        event_type: &str,
        status: Option<&str>,
        latency_ms: Option<u64>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO endpoint_history (endpoint_id, endpoint_name, url, event_type, status, latency_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#
        )
        .bind(endpoint_id)
        .bind(endpoint_name)
        .bind(url)
        .bind(event_type)
        .bind(status)
        .bind(latency_ms.map(|v| v as i64))
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// ------------------------------------------------------------------
// Internal row types for sqlx mapping
// ------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    parent_id: Option<String>,
    conversation_id: String,
    timestamp: String,
    sender: String,
    sender_ns: String,
    path: String,
    msg_type: String,
    data: String,
    headers: String,
    quantum: Option<String>,
}

impl TryFrom<MessageRow> for Message {
    type Error = anyhow::Error;

    fn try_from(row: MessageRow) -> Result<Self, Self::Error> {
        Ok(Message {
            id: row.id.parse()?,
            parent_id: row.parent_id.and_then(|s| s.parse().ok()),
            conversation_id: row.conversation_id.parse()?,
            timestamp: DateTime::parse_from_rfc3339(&row.timestamp)?.with_timezone(&Utc),
            sender: row.sender,
            sender_ns: row.sender_ns,
            path: row.path,
            msg_type: row.msg_type,
            data: serde_json::from_str(&row.data).unwrap_or_default(),
            headers: serde_json::from_str(&row.headers).unwrap_or_default(),
            quantum: row.quantum.and_then(|q| serde_json::from_str(&q).ok()),
        })
    }
}

#[derive(sqlx::FromRow)]
struct QuantumStateRow {
    endpoint_id: String,
    real_amplitude: f64,
    imag_amplitude: f64,
    pulls: i64,
    total_reward: f64,
}

impl Store {
    pub async fn insert_event(&self, event: &crate::event::Event) -> anyhow::Result<()> {
        let metadata = serde_json::to_string(&event.metadata).unwrap_or_default();

        sqlx::query(
            r#"
            INSERT INTO events (id, timestamp, kind, severity, source, message, conversation_id, endpoint_id, agent_id, metadata)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#
        )
        .bind(event.id.to_string())
        .bind(event.timestamp.to_rfc3339())
        .bind(format!("{:?}", event.kind))
        .bind(format!("{:?}", event.severity))
        .bind(&event.source)
        .bind(&event.message)
        .bind(event.conversation_id.map(|id| id.to_string()))
        .bind(event.endpoint_id.as_ref())
        .bind(event.agent_id.as_ref())
        .bind(metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
