use super::{
    stage_order::{OrderSide, StageOrderRequest},
    trade_preflight::{
        BrokerFormState, BrokerQuote, BrokerTradeConstraints, PreflightDecision, ReferenceQuote,
        SecurityIdentity,
    },
    trade_preflight_validation::{compare_quote, evaluate_order},
    types::WorkspaceKind,
};

fn order(side: OrderSide, price: &str, quantity: u64) -> StageOrderRequest {
    StageOrderRequest {
        security_code: "600028".to_string(),
        side,
        price: price.to_string(),
        quantity,
    }
}

fn form() -> BrokerFormState {
    BrokerFormState {
        price: Some("5.06".to_string()),
        quantity: Some(100),
        control_count: 2,
        security_code_editable: true,
    }
}

fn quote() -> BrokerQuote {
    BrokerQuote {
        latest_price: Some("5.06".to_string()),
        pre_close: Some("5.00".to_string()),
        up_limit: Some("5.50".to_string()),
        down_limit: Some("4.50".to_string()),
        buy_ceiling: None,
        sell_floor: None,
        bid_price_1: None,
        ask_price_1: None,
        trade_status: Some("正常".to_string()),
        captured_at: "now".to_string(),
        source: "broker_ui_ocr".to_string(),
    }
}

fn constraints() -> BrokerTradeConstraints {
    BrokerTradeConstraints {
        available_funds: Some("1000.00".to_string()),
        max_buy_quantity: Some(100),
        max_sell_quantity: Some(100),
    }
}

fn security() -> SecurityIdentity {
    SecurityIdentity {
        code: "600028".to_string(),
        name: "中国石化".to_string(),
    }
}

#[test]
fn no_order_only_observes_the_broker_state() {
    let mut warnings = Vec::new();
    let decision = evaluate_order(
        None,
        WorkspaceKind::Unknown,
        None,
        &BrokerFormState {
            price: None,
            quantity: None,
            control_count: 0,
            security_code_editable: false,
        },
        &quote(),
        &constraints(),
        None,
        &mut warnings,
    )
    .unwrap();
    assert_eq!(decision, PreflightDecision::ObservedOnly);
    assert!(warnings.is_empty());
}

#[test]
fn matching_buy_order_is_ready() {
    let mut warnings = Vec::new();
    let comparison = compare_quote(
        &ReferenceQuote {
            latest_price: "5.06".to_string(),
            source: "backend".to_string(),
            captured_at: None,
        },
        Some("5.06"),
    );
    let decision = evaluate_order(
        Some(&order(OrderSide::Buy, "5.06", 100)),
        WorkspaceKind::BuyOrder,
        Some(&security()),
        &form(),
        &quote(),
        &constraints(),
        Some(&comparison),
        &mut warnings,
    )
    .unwrap();
    assert_eq!(decision, PreflightDecision::Ready);
    assert!(warnings.is_empty());
}

#[test]
fn missing_broker_facts_need_review() {
    let mut incomplete_quote = quote();
    incomplete_quote.latest_price = None;
    incomplete_quote.trade_status = None;
    let mut warnings = Vec::new();
    let decision = evaluate_order(
        Some(&order(OrderSide::Buy, "5.06", 100)),
        WorkspaceKind::BuyOrder,
        Some(&security()),
        &form(),
        &incomplete_quote,
        &constraints(),
        None,
        &mut warnings,
    )
    .unwrap();
    assert_eq!(decision, PreflightDecision::NeedsReview);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("latest price"))
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("trade status"))
    );
}

#[test]
fn missing_reference_quote_needs_review() {
    let mut warnings = Vec::new();
    let decision = evaluate_order(
        Some(&order(OrderSide::Buy, "5.06", 100)),
        WorkspaceKind::BuyOrder,
        Some(&security()),
        &form(),
        &quote(),
        &constraints(),
        None,
        &mut warnings,
    )
    .unwrap();
    assert_eq!(decision, PreflightDecision::NeedsReview);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("reference quote"))
    );
}

#[test]
fn mismatched_workspace_and_limits_reject() {
    let mut warnings = Vec::new();
    let decision = evaluate_order(
        Some(&order(OrderSide::Buy, "5.06", 100)),
        WorkspaceKind::SellOrder,
        Some(&security()),
        &form(),
        &quote(),
        &constraints(),
        None,
        &mut warnings,
    )
    .unwrap();
    assert_eq!(decision, PreflightDecision::Rejected);

    let mut over_limit = quote();
    over_limit.up_limit = Some("5.05".to_string());
    warnings.clear();
    let decision = evaluate_order(
        Some(&order(OrderSide::Buy, "5.06", 100)),
        WorkspaceKind::BuyOrder,
        Some(&security()),
        &form(),
        &over_limit,
        &constraints(),
        None,
        &mut warnings,
    )
    .unwrap();
    assert_eq!(decision, PreflightDecision::Rejected);
}

#[test]
fn quote_comparison_marks_difference_and_unavailable() {
    let reference = ReferenceQuote {
        latest_price: "5.06".to_string(),
        source: "backend".to_string(),
        captured_at: None,
    };
    assert_eq!(compare_quote(&reference, Some("5.10")).status, "different");
    assert_eq!(compare_quote(&reference, None).status, "unavailable");
}

#[test]
fn invalid_reference_quote_is_degraded_not_fatal() {
    let mut warnings = Vec::new();
    let comparison = compare_quote(
        &ReferenceQuote {
            latest_price: "not-a-price".to_string(),
            source: "backend".to_string(),
            captured_at: None,
        },
        Some("5.06"),
    );
    assert_eq!(comparison.status, "invalid_reference");
    let decision = evaluate_order(
        Some(&order(OrderSide::Buy, "5.06", 100)),
        WorkspaceKind::BuyOrder,
        Some(&security()),
        &form(),
        &quote(),
        &constraints(),
        Some(&comparison),
        &mut warnings,
    )
    .unwrap();
    assert_eq!(decision, PreflightDecision::NeedsReview);
    assert!(warnings.iter().any(|warning| warning.contains("invalid")));
}
