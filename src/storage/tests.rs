#[cfg(test)]
mod integration {
    use super::super::Storage;
    use crate::pages::{OrderSide, StageOrderRequest};
    use crate::storage::redaction::redacted_order_summary;
    use chrono::Utc;
    use tempfile::tempdir;

    fn test_order() -> StageOrderRequest {
        StageOrderRequest {
            security_code: "600028".to_string(),
            side: OrderSide::Buy,
            price: "4.55".to_string(),
            quantity: 100,
        }
    }

    #[test]
    fn migrations_create_tables() {
        let storage = Storage::open_in_memory().unwrap();
        let conn = storage.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('operations','audit_events','operator_settings')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn prepare_and_idempotency_persists_across_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sqlite3");
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires = now + chrono::Duration::seconds(30);
        let payload = redacted_order_summary(&test_order());
        {
            let storage = Storage::open(&path).unwrap();
            storage
                .insert_prepare_operation(
                    &id,
                    super::super::LiveOperationKind::SubmitOrder,
                    "fp123",
                    "idem-key-1",
                    now,
                    expires,
                    &payload,
                    Some("http:test"),
                )
                .unwrap();
            assert!(storage.idempotency_exists("idem-key-1").unwrap());
        }
        {
            let storage = Storage::open(&path).unwrap();
            assert!(storage.idempotency_exists("idem-key-1").unwrap());
            let fetched = storage.get_operation(&id).unwrap().unwrap();
            assert_eq!(fetched.fingerprint, "fp123");
            assert!(!fetched.payload_summary.to_string().contains("token"));
            let audits = storage.list_audit_events(10, 0, Some(&id)).unwrap();
            assert!(audits.iter().any(|a| a.event_type == "prepare"));
            for audit in &audits {
                if let Some(detail) = &audit.detail {
                    assert!(!detail.to_string().to_lowercase().contains("token"));
                }
                assert!(!audit.fingerprint.to_lowercase().contains("token"));
            }
        }
    }

    #[test]
    fn db_file_does_not_contain_secrets() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("secretcheck.sqlite");
        let storage = Storage::open(&path).unwrap();
        let now = Utc::now();
        let expires = now + chrono::Duration::seconds(30);
        let payload = redacted_order_summary(&test_order());
        storage
            .insert_prepare_operation(
                &uuid::Uuid::new_v4().to_string(),
                super::super::LiveOperationKind::SubmitOrder,
                "fp-noleak",
                "key-noleak",
                now,
                expires,
                &payload,
                None,
            )
            .unwrap();
        drop(storage);
        let bytes = std::fs::read(&path).unwrap();
        let content = String::from_utf8_lossy(&bytes);
        for needle in ["STOCK_OPERATOR_AUTH_TOKEN", "confirmation_token", "Bearer"] {
            assert!(
                !content.contains(needle),
                "database file should not contain {needle}"
            );
        }
    }

    #[test]
    fn list_history_pagination() {
        let storage = Storage::open_in_memory().unwrap();
        let now = Utc::now();
        for i in 0..5 {
            let id = uuid::Uuid::new_v4().to_string();
            let payload = redacted_order_summary(&test_order());
            let created = now + chrono::Duration::milliseconds(i * 10);
            let expires = created + chrono::Duration::seconds(30);
            storage
                .insert_prepare_operation(
                    &id,
                    super::super::LiveOperationKind::SubmitOrder,
                    &format!("fp{i}"),
                    &format!("key-{i}"),
                    created,
                    expires,
                    &payload,
                    None,
                )
                .unwrap();
            storage.transition_to_confirming(&id, None).unwrap();
            storage.transition_to_confirmed(&id, None, None).unwrap();
        }
        let ops = storage.list_operations(3, 0, None, None).unwrap();
        assert_eq!(ops.len(), 3);
        let total = storage.count_operations(None, None).unwrap();
        assert_eq!(total, 5);
    }

    #[test]
    fn has_active_blocks_second_prepare() {
        let storage = Storage::open_in_memory().unwrap();
        let now = Utc::now();
        let expires = now + chrono::Duration::seconds(30);
        let payload = redacted_order_summary(&test_order());
        storage
            .insert_prepare_operation(
                &uuid::Uuid::new_v4().to_string(),
                super::super::LiveOperationKind::SubmitOrder,
                "fp1",
                "key-1",
                now,
                expires,
                &payload,
                None,
            )
            .unwrap();
        assert!(storage.has_active_live_operation().unwrap());
    }

    #[test]
    fn expiry_transitions_to_expired_with_audit() {
        let storage = Storage::open_in_memory().unwrap();
        let now = Utc::now() - chrono::Duration::seconds(60);
        let expires = now + chrono::Duration::seconds(30);
        let payload = redacted_order_summary(&test_order());
        let id = uuid::Uuid::new_v4().to_string();
        storage
            .insert_prepare_operation(
                &id,
                super::super::LiveOperationKind::SubmitOrder,
                "fp-exp",
                "key-exp",
                now,
                expires,
                &payload,
                None,
            )
            .unwrap();
        let n = storage.expire_stale_operations().unwrap();
        assert_eq!(n, 1);
        let op = storage.get_operation(&id).unwrap().unwrap();
        assert_eq!(op.state, super::super::LiveOperationState::Expired);
        let audits = storage.list_audit_events(10, 0, Some(&id)).unwrap();
        assert!(audits.iter().any(|a| a.event_type == "expired"));
        assert!(storage.has_active_live_operation().unwrap());
    }

    #[test]
    fn reconcile_pending_to_unknown_or_expired() {
        let storage = Storage::open_in_memory().unwrap();
        let now = Utc::now();
        let future_exp = now + chrono::Duration::seconds(120);
        let past_exp = now - chrono::Duration::seconds(120);
        let payload = redacted_order_summary(&test_order());
        let id_future = uuid::Uuid::new_v4().to_string();
        let id_past = uuid::Uuid::new_v4().to_string();
        storage
            .insert_prepare_operation(
                &id_future,
                super::super::LiveOperationKind::SubmitOrder,
                "fp-future",
                "key-future",
                now,
                future_exp,
                &payload,
                None,
            )
            .unwrap();
        storage
            .insert_prepare_operation(
                &id_past,
                super::super::LiveOperationKind::CancelOrder,
                "fp-past",
                "key-past",
                past_exp - chrono::Duration::seconds(10),
                past_exp,
                &payload,
                None,
            )
            .unwrap();
        let reconciled = storage.reconcile_pending_operations().unwrap();
        assert!(reconciled >= 1);
        let op_future = storage.get_operation(&id_future).unwrap().unwrap();
        let op_past = storage.get_operation(&id_past).unwrap().unwrap();
        assert_eq!(op_future.state, super::super::LiveOperationState::Unknown);
        assert_eq!(op_past.state, super::super::LiveOperationState::Expired);
    }
}
