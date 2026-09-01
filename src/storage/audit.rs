use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;

use super::{Storage, parse_time};

#[derive(Clone, Debug)]
pub struct AuditEvent {
    pub id: i64,
    pub operation_id: String,
    pub event_type: String,
    pub from_state: Option<String>,
    pub to_state: String,
    pub created_at: DateTime<Utc>,
    pub actor_source: Option<String>,
    pub detail: Option<serde_json::Value>,
    pub fingerprint: String,
}

impl Storage {
    pub fn list_audit_events(
        &self,
        limit: usize,
        offset: usize,
        operation_id: Option<&str>,
    ) -> Result<Vec<AuditEvent>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut sql = String::from(
            "SELECT id, operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint FROM audit_events",
        );
        if operation_id.is_some() {
            sql.push_str(" WHERE operation_id = ?1");
            sql.push_str(" ORDER BY created_at DESC LIMIT ?2 OFFSET ?3");
        } else {
            sql.push_str(" ORDER BY created_at DESC LIMIT ?1 OFFSET ?2");
        }
        let mut stmt = conn.prepare(&sql).context("prepare list_audit")?;
        let rows = if let Some(op_id) = operation_id {
            stmt.query_map(
                params![op_id, limit as i64, offset as i64],
                audit_row_mapper,
            )
        } else {
            stmt.query_map(params![limit as i64, offset as i64], audit_row_mapper)
        }
        .context("query audit")?
        .collect::<Result<Vec<_>, _>>()
        .context("collect audit")?;
        Ok(rows)
    }

    pub fn count_audit_events(&self, operation_id: Option<&str>) -> Result<i64> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        if let Some(op_id) = operation_id {
            let c: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE operation_id = ?1",
                    params![op_id],
                    |row| row.get(0),
                )
                .context("count audit filtered")?;
            Ok(c)
        } else {
            let c: i64 = conn
                .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
                .context("count audit")?;
            Ok(c)
        }
    }

    #[allow(dead_code)]
    pub fn get_audit_event(&self, id: i64) -> Result<Option<AuditEvent>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT id, operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint FROM audit_events WHERE id = ?1")
            .context("prepare get_audit")?;
        let row = stmt
            .query_row(params![id], audit_row_mapper)
            .optional()
            .context("get audit")?;
        Ok(row)
    }
}

fn audit_row_mapper(row: &rusqlite::Row) -> rusqlite::Result<AuditEvent> {
    let id: i64 = row.get(0)?;
    let operation_id: String = row.get(1)?;
    let event_type: String = row.get(2)?;
    let from_state: Option<String> = row.get(3)?;
    let to_state: String = row.get(4)?;
    let created_at_s: String = row.get(5)?;
    let actor_source: Option<String> = row.get(6)?;
    let detail_s: Option<String> = row.get(7)?;
    let fingerprint: String = row.get(8)?;
    let created_at = parse_time(&created_at_s)
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    let detail: Option<serde_json::Value> =
        detail_s.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s)));
    Ok(AuditEvent {
        id,
        operation_id,
        event_type,
        from_state,
        to_state,
        created_at,
        actor_source,
        detail,
        fingerprint,
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
