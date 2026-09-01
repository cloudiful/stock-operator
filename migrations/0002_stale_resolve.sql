-- 0002_stale_resolve.sql: add distinct stale_resolved audit event for supervised desktop recovery
-- Existing operations already support 'aborted' terminal; stale recovery writes aborted state with stale_resolved event.

PRAGMA foreign_keys=OFF;

-- Recreate audit_events to allow 'stale_resolved' event type (distinct from abort for audit clarity).
CREATE TABLE IF NOT EXISTS audit_events_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK (event_type IN ('prepare', 'confirm_start', 'confirmed', 'abort', 'expired', 'unknown', 'stale_resolved')),
    from_state TEXT,
    to_state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    actor_source TEXT,
    detail TEXT,
    fingerprint TEXT NOT NULL
) STRICT;

INSERT INTO audit_events_new (id, operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint)
    SELECT id, operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint FROM audit_events;

DROP TABLE IF EXISTS audit_events;

ALTER TABLE audit_events_new RENAME TO audit_events;

CREATE INDEX IF NOT EXISTS idx_audit_events_operation_id ON audit_events(operation_id);
CREATE INDEX IF NOT EXISTS idx_audit_events_created_at ON audit_events(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_event_type ON audit_events(event_type);

PRAGMA foreign_keys=ON;
