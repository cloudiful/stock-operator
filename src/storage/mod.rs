use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;

pub mod audit;
pub mod operations;
pub mod redaction;
pub mod settings;
pub mod transitions;

#[cfg(test)]
mod tests;

pub use audit::AuditEvent;
pub use operations::OperationRecord;
pub use redaction::{redacted_cancellation_summary, redacted_order_summary};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveOperationKind {
    SubmitOrder,
    CancelOrder,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveOperationState {
    ConfirmationOpened,
    Confirming,
    Confirmed,
    Unknown,
    Expired,
    Aborted,
}

#[derive(Clone)]
pub struct Storage {
    pub(crate) conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create database directory {}", parent.display())
                })?;
            }
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open SQLite at {}", path.display()))?;
        Self::configure(&conn)?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: path.to_path_buf(),
        };
        storage.apply_migrations()?;
        storage.reconcile_pending_operations()?;
        let _ = storage.ensure_instance_id(None)?;
        Ok(storage)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("failed to open in-memory SQLite")?;
        Self::configure(&conn)?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: PathBuf::from(":memory:"),
        };
        storage.apply_migrations()?;
        storage.reconcile_pending_operations()?;
        Ok(storage)
    }

    fn configure(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "journal_mode", "WAL")
            .context("failed to set journal_mode")?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .context("failed to enable foreign_keys")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .context("failed to set busy_timeout")?;
        Ok(())
    }

    fn apply_migrations(&self) -> Result<()> {
        let sql = include_str!("../../migrations/0001_initial.sql");
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute_batch(sql)
            .context("failed to apply SQLite migrations")?;
        Ok(())
    }

    pub fn reconcile_pending_operations(&self) -> Result<usize> {
        let now = Utc::now();
        let now_str = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT id, expires_at FROM operations WHERE state IN ('confirmation_opened','confirming')")
            .context("prepare reconcile query")?;
        let pending: Vec<(String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .context("query pending operations")?
            .collect::<Result<Vec<_>, _>>()
            .context("collect pending")?;
        drop(stmt);
        let mut reconciled = 0;
        for (id, expires_at_str) in pending {
            let expires_at = parse_time(&expires_at_str).unwrap_or(now);
            let (new_state, event_type) = if now >= expires_at {
                ("expired", "expired")
            } else {
                ("unknown", "unknown")
            };
            let from_state: String = conn
                .query_row(
                    "SELECT state FROM operations WHERE id = ?1",
                    rusqlite::params![id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| "confirmation_opened".to_string());
            let updated = conn
                .execute(
                    "UPDATE operations SET state = ?1, updated_at = ?2 WHERE id = ?3 AND state IN ('confirmation_opened','confirming')",
                    rusqlite::params![new_state, now_str, id],
                )
                .context("reconcile update")?;
            if updated == 0 {
                continue;
            }
            let fingerprint: String = conn
                .query_row(
                    "SELECT fingerprint FROM operations WHERE id = ?1",
                    rusqlite::params![id],
                    |row| row.get(0),
                )
                .unwrap_or_default();
            let detail =
                serde_json::json!({"reason":"startup_reconcile","previous_state":from_state});
            conn.execute(
                "INSERT INTO audit_events (operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![id, event_type, from_state, new_state, now_str, "system:startup", detail.to_string(), fingerprint],
            )
            .context("insert reconcile audit")?;
            reconciled += 1;
        }
        Ok(reconciled)
    }

    pub fn expire_stale_operations(&self) -> Result<usize> {
        let now = Utc::now();
        let now_str = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT id, state, fingerprint, expires_at FROM operations WHERE state IN ('confirmation_opened','confirming')")
            .context("prepare expire query")?;
        let rows: Vec<(String, String, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .context("query expire candidates")?
            .collect::<Result<Vec<_>, _>>()
            .context("collect expire candidates")?;
        drop(stmt);
        let mut count = 0;
        for (id, state, fingerprint, expires_at_str) in rows {
            let expires_at = parse_time(&expires_at_str).unwrap_or(now);
            if now < expires_at {
                continue;
            }
            let updated = conn
                .execute(
                    "UPDATE operations SET state = 'expired', updated_at = ?1 WHERE id = ?2 AND state = ?3",
                    rusqlite::params![now_str, id, state],
                )
                .context("expire update")?;
            if updated == 0 {
                continue;
            }
            let detail = serde_json::json!({"reason":"ttl_expired","previous_state":state});
            conn.execute(
                "INSERT INTO audit_events (operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint) VALUES (?1,'expired',?2,'expired',?3,?4,?5,?6)",
                rusqlite::params![id, state, now_str, "system:expiry", detail.to_string(), fingerprint],
            )
            .context("insert expiry audit")?;
            count += 1;
        }
        Ok(count)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn parse_time(s: &str) -> Result<DateTime<Utc>> {
    let dt = DateTime::parse_from_rfc3339(s)
        .with_context(|| format!("invalid RFC3339 timestamp: {s}"))?
        .with_timezone(&Utc);
    Ok(dt)
}

pub(crate) fn kind_to_str(kind: LiveOperationKind) -> &'static str {
    match kind {
        LiveOperationKind::SubmitOrder => "submit_order",
        LiveOperationKind::CancelOrder => "cancel_order",
    }
}

pub(crate) fn parse_kind(s: &str) -> Result<LiveOperationKind> {
    match s {
        "submit_order" => Ok(LiveOperationKind::SubmitOrder),
        "cancel_order" => Ok(LiveOperationKind::CancelOrder),
        _ => anyhow::bail!("unknown operation kind: {s}"),
    }
}

pub(crate) fn state_to_str(state: LiveOperationState) -> &'static str {
    match state {
        LiveOperationState::ConfirmationOpened => "confirmation_opened",
        LiveOperationState::Confirming => "confirming",
        LiveOperationState::Confirmed => "confirmed",
        LiveOperationState::Unknown => "unknown",
        LiveOperationState::Expired => "expired",
        LiveOperationState::Aborted => "aborted",
    }
}

pub(crate) fn parse_state(s: &str) -> Result<LiveOperationState> {
    match s {
        "confirmation_opened" => Ok(LiveOperationState::ConfirmationOpened),
        "confirming" => Ok(LiveOperationState::Confirming),
        "confirmed" => Ok(LiveOperationState::Confirmed),
        "unknown" => Ok(LiveOperationState::Unknown),
        "expired" => Ok(LiveOperationState::Expired),
        "aborted" => Ok(LiveOperationState::Aborted),
        _ => anyhow::bail!("unknown operation state: {s}"),
    }
}
