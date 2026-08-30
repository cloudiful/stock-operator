use anyhow::{Context, Result, bail};
use chrono::Utc;
use rusqlite::params;

use super::{Storage, parse_time};

impl Storage {
    pub fn transition_to_confirming(&self, id: &str, actor_source: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("BEGIN IMMEDIATE", [])
            .context("begin confirming")?;
        let res: Result<()> = (|| {
            let current: (String, String) = conn
                .query_row(
                    "SELECT state, fingerprint FROM operations WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .with_context(|| format!("operation {id} not found for confirming"))?;
            if current.0 != "confirmation_opened" {
                bail!("operation is not awaiting confirmation");
            }
            let expires_at_str: String = conn.query_row(
                "SELECT expires_at FROM operations WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )?;
            let expires_at = parse_time(&expires_at_str)?;
            if Utc::now() >= expires_at {
                conn.execute(
                    "UPDATE operations SET state='expired', updated_at=?1 WHERE id=?2",
                    params![now, id],
                )?;
                let detail = serde_json::json!({"reason":"ttl_expired_during_confirm"});
                conn.execute(
                    "INSERT INTO audit_events (operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint) VALUES (?1,'expired',?2,'expired',?3,?4,?5,?6)",
                    params![id, current.0, now, actor_source, detail.to_string(), current.1],
                )?;
                bail!("operation has expired");
            }
            conn.execute(
                "UPDATE operations SET state='confirming', updated_at=?1 WHERE id=?2 AND state='confirmation_opened'",
                params![now, id],
            )
            .context("update to confirming")?;
            let detail = serde_json::json!({"action":"confirm_start"});
            conn.execute(
                "INSERT INTO audit_events (operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint) VALUES (?1,'confirm_start','confirmation_opened','confirming',?2,?3,?4,?5)",
                params![id, now, actor_source, detail.to_string(), current.1],
            )
            .context("insert confirm_start audit")?;
            Ok(())
        })();
        if res.is_err() {
            let _ = conn.execute("ROLLBACK", []);
            return res;
        }
        conn.execute("COMMIT", []).context("commit confirming")?;
        Ok(())
    }

    pub fn transition_to_confirmed(
        &self,
        id: &str,
        result_summary: Option<&serde_json::Value>,
        actor_source: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("BEGIN IMMEDIATE", [])
            .context("begin confirmed")?;
        let res: Result<()> = (|| {
            let (state, fingerprint): (String, String) = conn.query_row(
                "SELECT state, fingerprint FROM operations WHERE id=?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if state != "confirming" {
                bail!("operation is not in confirming state");
            }
            let result_str = result_summary.map(|v| v.to_string());
            conn.execute(
                "UPDATE operations SET state='confirmed', updated_at=?1, result_summary=?2 WHERE id=?3",
                params![now, result_str, id],
            )?;
            let detail = result_summary
                .cloned()
                .unwrap_or(serde_json::json!({"outcome":"confirmed"}));
            conn.execute(
                "INSERT INTO audit_events (operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint) VALUES (?1,'confirmed','confirming','confirmed',?2,?3,?4,?5)",
                params![id, now, actor_source, detail.to_string(), fingerprint],
            )?;
            Ok(())
        })();
        if res.is_err() {
            let _ = conn.execute("ROLLBACK", []);
            return res;
        }
        conn.execute("COMMIT", []).context("commit confirmed")?;
        Ok(())
    }

    pub fn transition_to_unknown(
        &self,
        id: &str,
        detail: &serde_json::Value,
        actor_source: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("BEGIN IMMEDIATE", [])
            .context("begin unknown")?;
        let res: Result<()> = (|| {
            let (state, fingerprint): (String, String) = conn.query_row(
                "SELECT state, fingerprint FROM operations WHERE id=?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if matches!(state.as_str(), "unknown" | "confirmed" | "aborted") {
                bail!("operation cannot transition to unknown from {}", state);
            }
            conn.execute(
                "UPDATE operations SET state='unknown', updated_at=?1, result_summary=?2 WHERE id=?3",
                params![now, detail.to_string(), id],
            )?;
            conn.execute(
                "INSERT INTO audit_events (operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint) VALUES (?1,'unknown',?2,'unknown',?3,?4,?5,?6)",
                params![id, state, now, actor_source, detail.to_string(), fingerprint],
            )?;
            Ok(())
        })();
        if res.is_err() {
            let _ = conn.execute("ROLLBACK", []);
            return res;
        }
        conn.execute("COMMIT", []).context("commit unknown")?;
        Ok(())
    }

    pub fn transition_to_aborted(&self, id: &str, actor_source: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("BEGIN IMMEDIATE", [])
            .context("begin aborted")?;
        let res: Result<()> = (|| {
            let (state, fingerprint): (String, String) = conn.query_row(
                "SELECT state, fingerprint FROM operations WHERE id=?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if !matches!(
                state.as_str(),
                "confirmation_opened" | "expired" | "unknown"
            ) {
                bail!("operation cannot be aborted in its current state");
            }
            conn.execute(
                "UPDATE operations SET state='aborted', updated_at=?1 WHERE id=?2",
                params![now, id],
            )?;
            let detail = serde_json::json!({"previous_state": state, "action":"abort"});
            conn.execute(
                "INSERT INTO audit_events (operation_id, event_type, from_state, to_state, created_at, actor_source, detail, fingerprint) VALUES (?1,'abort',?2,'aborted',?3,?4,?5,?6)",
                params![id, state, now, actor_source, detail.to_string(), fingerprint],
            )?;
            Ok(())
        })();
        if res.is_err() {
            let _ = conn.execute("ROLLBACK", []);
            return res;
        }
        conn.execute("COMMIT", []).context("commit aborted")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::Storage;
    use crate::pages::{OrderSide, StageOrderRequest};
    use crate::storage::redaction::redacted_order_summary;
    use chrono::Utc;

    fn order() -> StageOrderRequest {
        StageOrderRequest {
            security_code: "600028".to_string(),
            side: OrderSide::Buy,
            price: "4.55".to_string(),
            quantity: 100,
        }
    }

    #[test]
    fn confirm_flow_and_audit() {
        let storage = Storage::open_in_memory().unwrap();
        let now = Utc::now();
        let expires = now + chrono::Duration::seconds(30);
        let payload = redacted_order_summary(&order());
        let id = uuid::Uuid::new_v4().to_string();
        storage
            .insert_prepare_operation(
                &id,
                super::super::LiveOperationKind::SubmitOrder,
                "fp-confirm",
                "key-confirm",
                now,
                expires,
                &payload,
                Some("http"),
            )
            .unwrap();
        storage.transition_to_confirming(&id, Some("http")).unwrap();
        let op = storage.get_operation(&id).unwrap().unwrap();
        assert_eq!(op.state, super::super::LiveOperationState::Confirming);
        storage
            .transition_to_confirmed(&id, Some(&serde_json::json!({"ok":true})), Some("http"))
            .unwrap();
        let op2 = storage.get_operation(&id).unwrap().unwrap();
        assert_eq!(op2.state, super::super::LiveOperationState::Confirmed);
        let audits = storage.list_audit_events(10, 0, Some(&id)).unwrap();
        assert_eq!(audits.len(), 3);
        for a in &audits {
            if let Some(d) = &a.detail {
                assert!(!d.to_string().to_lowercase().contains("token"));
            }
        }
    }

    #[test]
    fn unknown_and_abort_flow() {
        let storage = Storage::open_in_memory().unwrap();
        let now = Utc::now();
        let expires = now + chrono::Duration::seconds(30);
        let payload = redacted_order_summary(&order());
        let id = uuid::Uuid::new_v4().to_string();
        storage
            .insert_prepare_operation(
                &id,
                super::super::LiveOperationKind::SubmitOrder,
                "fp-unk",
                "key-unk",
                now,
                expires,
                &payload,
                None,
            )
            .unwrap();
        storage.transition_to_confirming(&id, None).unwrap();
        storage
            .transition_to_unknown(&id, &serde_json::json!({"error":"broker ui failed"}), None)
            .unwrap();
        let op = storage.get_operation(&id).unwrap().unwrap();
        assert_eq!(op.state, super::super::LiveOperationState::Unknown);
        storage.transition_to_aborted(&id, None).unwrap();
        let op2 = storage.get_operation(&id).unwrap().unwrap();
        assert_eq!(op2.state, super::super::LiveOperationState::Aborted);
        assert!(!storage.has_active_live_operation().unwrap());
    }

    #[test]
    fn unknown_guard_rejects_terminal_states() {
        let storage = Storage::open_in_memory().unwrap();
        let now = Utc::now();
        let expires = now + chrono::Duration::seconds(30);
        let payload = redacted_order_summary(&order());
        let id = uuid::Uuid::new_v4().to_string();
        storage
            .insert_prepare_operation(
                &id,
                super::super::LiveOperationKind::SubmitOrder,
                "fp-guard",
                "key-guard",
                now,
                expires,
                &payload,
                None,
            )
            .unwrap();
        storage.transition_to_confirming(&id, None).unwrap();
        storage.transition_to_confirmed(&id, None, None).unwrap();
        let err = storage
            .transition_to_unknown(&id, &serde_json::json!({"x":1}), None)
            .unwrap_err();
        assert!(err.to_string().contains("cannot transition to unknown"));
    }
}
