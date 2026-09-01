use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;

use super::{
    LiveOperationKind, LiveOperationState, Storage, parse_kind, parse_state, parse_time,
    state_to_str,
};

use super::kind_to_str;

#[derive(Clone, Debug)]
pub struct OperationRecord {
    pub id: String,
    pub kind: LiveOperationKind,
    pub state: LiveOperationState,
    pub fingerprint: String,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub payload_summary: serde_json::Value,
    pub result_summary: Option<serde_json::Value>,
    pub actor_source: Option<String>,
}

impl Storage {
    pub fn has_active_live_operation(&self) -> Result<bool> {
        self.expire_stale_operations()?;
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM operations WHERE state IN ('confirmation_opened','confirming','expired','unknown')",
                [],
                |row| row.get(0),
            )
            .context("has_active query")?;
        Ok(count > 0)
    }

    pub fn idempotency_exists(&self, key: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM operations WHERE idempotency_key = ?1",
                params![key],
                |row| row.get(0),
            )
            .context("idempotency_exists query")?;
        Ok(count > 0)
    }

    #[allow(dead_code)]
    pub fn get_idempotency_operation_id(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM operations WHERE idempotency_key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .context("get idempotency")?;
        Ok(id)
    }

    pub fn insert_prepare_operation(
        &self,
        id: &str,
        kind: LiveOperationKind,
        fingerprint: &str,
        idempotency_key: &str,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        payload_summary: &serde_json::Value,
        actor_source: Option<&str>,
    ) -> Result<()> {
        let now_str = created_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let expires_str = expires_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let kind_str = kind_to_str(kind);
        let payload_str = serde_json::to_string(payload_summary).context("serialize payload")?;
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("BEGIN IMMEDIATE", [])
            .context("begin transaction")?;
        let op_result = (|| -> Result<()> {
            conn.execute(
                "INSERT INTO operations (id, kind, state, fingerprint, idempotency_key, created_at, expires_at, updated_at, payload_summary, actor_source) VALUES (?1,?2,'confirmation_opened',?3,?4,?5,?6,?7,?8,?9)",
                params![id, kind_str, fingerprint, idempotency_key, now_str, expires_str, now_str, payload_str, actor_source],
            )
            .with_context(|| format!("insert operation {id}"))?;
            let detail = serde_json::json!({"kind":kind_str, "payload": payload_summary});
            conn.execute(
                "INSERT INTO audit_events (operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint) VALUES (?1,'prepare',NULL,'confirmation_opened',?2,?3,?4,?5)",
                params![id, now_str, actor_source, detail.to_string(), fingerprint],
            )
            .context("insert prepare audit")?;
            Ok(())
        })();
        if op_result.is_err() {
            let _ = conn.execute("ROLLBACK", []);
            return op_result;
        }
        conn.execute("COMMIT", []).context("commit prepare")?;
        Ok(())
    }

    pub fn get_operation(&self, id: &str) -> Result<Option<OperationRecord>> {
        self.expire_stale_operations()?;
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, state, fingerprint, idempotency_key, created_at, expires_at, updated_at, payload_summary, result_summary, actor_source FROM operations WHERE id = ?1",
            )
            .context("prepare get_operation")?;
        let row = stmt
            .query_row(params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            })
            .optional()
            .context("query get_operation")?;
        drop(stmt);
        match row {
            None => Ok(None),
            Some((
                id,
                kind_s,
                state_s,
                fingerprint,
                idempotency_key,
                created_at_s,
                expires_at_s,
                updated_at_s,
                payload_s,
                result_s,
                actor_source,
            )) => {
                let kind = parse_kind(&kind_s)?;
                let state = parse_state(&state_s)?;
                let created_at = parse_time(&created_at_s)?;
                let expires_at = parse_time(&expires_at_s)?;
                let updated_at = parse_time(&updated_at_s)?;
                let payload_summary: serde_json::Value =
                    serde_json::from_str(&payload_s).unwrap_or(serde_json::Value::Null);
                let result_summary: Option<serde_json::Value> = match result_s {
                    Some(s) => {
                        Some(serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s)))
                    }
                    None => None,
                };
                Ok(Some(OperationRecord {
                    id,
                    kind,
                    state,
                    fingerprint,
                    idempotency_key,
                    created_at,
                    expires_at,
                    updated_at,
                    payload_summary,
                    result_summary,
                    actor_source,
                }))
            }
        }
    }

    pub fn list_operations(
        &self,
        limit: usize,
        offset: usize,
        kind_filter: Option<LiveOperationKind>,
        state_filter: Option<LiveOperationState>,
    ) -> Result<Vec<OperationRecord>> {
        self.expire_stale_operations()?;
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut sql = String::from(
            "SELECT id, kind, state, fingerprint, idempotency_key, created_at, expires_at, updated_at, payload_summary, result_summary, actor_source FROM operations",
        );
        let mut conditions = Vec::new();
        if kind_filter.is_some() {
            conditions.push("kind = ?");
        }
        if state_filter.is_some() {
            conditions.push("state = ?");
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        let mut stmt = conn.prepare(&sql).context("prepare list_operations")?;
        let rows = match (kind_filter, state_filter) {
            (Some(k), Some(s)) => {
                let kind_s = kind_to_str(k);
                let state_s = state_to_str(s);
                stmt.query_map(
                    params![kind_s, state_s, limit as i64, offset as i64],
                    row_mapper,
                )
            }
            (Some(k), None) => {
                let kind_s = kind_to_str(k);
                stmt.query_map(params![kind_s, limit as i64, offset as i64], row_mapper)
            }
            (None, Some(s)) => {
                let state_s = state_to_str(s);
                stmt.query_map(params![state_s, limit as i64, offset as i64], row_mapper)
            }
            (None, None) => stmt.query_map(params![limit as i64, offset as i64], row_mapper),
        }
        .context("query list_operations")?
        .collect::<Result<Vec<_>, _>>()
        .context("collect list_operations")?;
        Ok(rows)
    }

    pub fn count_operations(
        &self,
        kind_filter: Option<LiveOperationKind>,
        state_filter: Option<LiveOperationState>,
    ) -> Result<i64> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut sql = String::from("SELECT COUNT(*) FROM operations");
        let mut conditions = Vec::new();
        if kind_filter.is_some() {
            conditions.push("kind = ?");
        }
        if state_filter.is_some() {
            conditions.push("state = ?");
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let count: i64 = match (kind_filter, state_filter) {
            (Some(k), Some(s)) => {
                let kind_s = kind_to_str(k);
                let state_s = state_to_str(s);
                conn.query_row(&sql, params![kind_s, state_s], |row| row.get(0))
                    .context("count operations")?
            }
            (Some(k), None) => {
                let kind_s = kind_to_str(k);
                conn.query_row(&sql, params![kind_s], |row| row.get(0))
                    .context("count operations")?
            }
            (None, Some(s)) => {
                let state_s = state_to_str(s);
                conn.query_row(&sql, params![state_s], |row| row.get(0))
                    .context("count operations")?
            }
            (None, None) => conn
                .query_row(&sql, [], |row| row.get(0))
                .context("count operations")?,
        };
        Ok(count)
    }
}

fn row_mapper(row: &rusqlite::Row) -> rusqlite::Result<OperationRecord> {
    let id: String = row.get(0)?;
    let kind_s: String = row.get(1)?;
    let state_s: String = row.get(2)?;
    let fingerprint: String = row.get(3)?;
    let idempotency_key: Option<String> = row.get(4)?;
    let created_at_s: String = row.get(5)?;
    let expires_at_s: String = row.get(6)?;
    let updated_at_s: String = row.get(7)?;
    let payload_s: String = row.get(8)?;
    let result_s: Option<String> = row.get(9)?;
    let actor_source: Option<String> = row.get(10)?;

    let kind =
        parse_kind(&kind_s).map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    let state =
        parse_state(&state_s).map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    let created_at = parse_time(&created_at_s)
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    let expires_at = parse_time(&expires_at_s)
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    let updated_at = parse_time(&updated_at_s)
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    let payload_summary: serde_json::Value =
        serde_json::from_str(&payload_s).unwrap_or(serde_json::Value::Null);
    let result_summary: Option<serde_json::Value> = match result_s {
        Some(s) => Some(serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s))),
        None => None,
    };
    Ok(OperationRecord {
        id,
        kind,
        state,
        fingerprint,
        idempotency_key,
        created_at,
        expires_at,
        updated_at,
        payload_summary,
        result_summary,
        actor_source,
    })
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
