-- Xfiles SQLite schema

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    parent_id TEXT,
    conversation_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    sender TEXT NOT NULL,
    sender_ns TEXT NOT NULL,
    path TEXT NOT NULL,
    msg_type TEXT NOT NULL,
    data TEXT,
    headers TEXT,
    quantum TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id);
CREATE INDEX idx_messages_sender ON messages(sender);

CREATE TABLE IF NOT EXISTS quantum_state (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL,
    endpoint_id TEXT NOT NULL,
    real_amplitude REAL NOT NULL DEFAULT 1.0,
    imag_amplitude REAL NOT NULL DEFAULT 0.0,
    pulls INTEGER NOT NULL DEFAULT 0,
    total_reward REAL NOT NULL DEFAULT 0.0,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(conversation_id, endpoint_id)
);

CREATE INDEX idx_quantum_conversation ON quantum_state(conversation_id);

CREATE TABLE IF NOT EXISTS feedback_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL,
    endpoint_id TEXT NOT NULL,
    success INTEGER NOT NULL,
    latency_ms INTEGER NOT NULL,
    quality_score REAL,
    error_kind TEXT,
    metadata TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_feedback_endpoint ON feedback_events(endpoint_id);
CREATE INDEX idx_feedback_message ON feedback_events(message_id);

CREATE TABLE IF NOT EXISTS endpoint_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint_id TEXT NOT NULL,
    endpoint_name TEXT NOT NULL,
    url TEXT NOT NULL,
    event_type TEXT NOT NULL,
    status TEXT,
    latency_ms INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_endpoint_history_id ON endpoint_history(endpoint_id);
