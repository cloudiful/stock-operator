-- 0001_initial.sql: SQLite foundation for stock-operator
-- Stores ordinary operator settings, live operations, and audit events.
-- Secrets (bearer tokens, confirmation tokens) must never be stored.

PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS operator_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS operations (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('submit_order', 'cancel_order')),
    state TEXT NOT NULL CHECK (state IN ('confirmation_opened', 'confirming', 'confirmed', 'unknown', 'expired', 'aborted')),
    fingerprint TEXT NOT NULL,
    idempotency_key TEXT UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    payload_summary TEXT NOT NULL,
    result_summary TEXT,
    actor_source TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_operations_state ON operations(state);
CREATE INDEX IF NOT EXISTS idx_operations_created_at ON operations(created_at);
CREATE INDEX IF NOT EXISTS idx_operations_expires_at ON operations(expires_at);
CREATE INDEX IF NOT EXISTS idx_operations_kind ON operations(kind);

CREATE TABLE IF NOT EXISTS audit_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK (event_type IN ('prepare', 'confirm_start', 'confirmed', 'abort', 'expired', 'unknown')),
    from_state TEXT,
    to_state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    actor_source TEXT,
    detail TEXT,
    fingerprint TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_audit_events_operation_id ON audit_events(operation_id);
CREATE INDEX IF NOT EXISTS idx_audit_events_created_at ON audit_events(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_event_type ON audit_events(event_type);
