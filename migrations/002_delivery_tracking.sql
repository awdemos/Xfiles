-- Add delivery tracking to messages and conversation listing support

ALTER TABLE messages ADD COLUMN delivery_status TEXT DEFAULT 'pending';
ALTER TABLE messages ADD COLUMN delivered_at TEXT;

CREATE INDEX idx_messages_delivery ON messages(delivery_status);

-- Table to track message delivery attempts
CREATE TABLE IF NOT EXISTS delivery_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL,
    endpoint_id TEXT,
    agent_id TEXT,
    status TEXT NOT NULL,
    error TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_delivery_attempts_message ON delivery_attempts(message_id);
