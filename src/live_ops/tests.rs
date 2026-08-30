use crate::operator_types::OperatorError;
use crate::storage::redacted_order_summary;
use crate::{
    pages::{CancellationTarget, OrderSide, StageOrderRequest},
    storage::{
        LiveOperationKind,
        redaction::{LivePayload, fingerprint, reconstruct_cancellation, reconstruct_order},
    },
};

fn order() -> StageOrderRequest {
    StageOrderRequest {
        security_code: "600028".to_string(),
        side: OrderSide::Buy,
        price: "4.55".to_string(),
        quantity: 100,
    }
}

#[test]
fn fingerprint_is_stable_and_payload_bound() {
    let first = fingerprint(LiveOperationKind::SubmitOrder, &LivePayload::Order(order())).unwrap();
    let second = fingerprint(LiveOperationKind::SubmitOrder, &LivePayload::Order(order())).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
    assert_ne!(
        first,
        fingerprint(
            LiveOperationKind::CancelOrder,
            &LivePayload::Cancellation(CancellationTarget {
                contract_id: "3506784".to_string(),
                security_code: "600028".to_string(),
                security_name: "中国石化".to_string(),
                side: "买入".to_string(),
                price: "4.55".to_string(),
                quantity: 100
            })
        )
        .unwrap()
    );
}

#[test]
fn reconstruct_order_roundtrip() {
    let summary = redacted_order_summary(&order());
    let rebuilt = reconstruct_order(&summary).unwrap();
    assert_eq!(rebuilt.security_code, order().security_code);
    assert_eq!(rebuilt.price, order().price);
    assert_eq!(rebuilt.quantity, order().quantity);
}

#[test]
fn reconstruct_cancellation_roundtrip() {
    let target = CancellationTarget {
        contract_id: "3506784".to_string(),
        security_code: "600028".to_string(),
        security_name: "中国石化".to_string(),
        side: "买入".to_string(),
        price: "4.55".to_string(),
        quantity: 100,
    };
    let summary = crate::storage::redacted_cancellation_summary(&target);
    let rebuilt = reconstruct_cancellation(&summary).unwrap();
    assert_eq!(rebuilt.contract_id, target.contract_id);
}

#[test]
fn typed_error_mapping_preserves_conflict_semantics() {
    let err = OperatorError::IdempotencyConflict;
    assert!(err.to_string().contains("already been used"));
    let err2 = OperatorError::ActiveConflict;
    assert!(err2.to_string().contains("another live operation"));
}
