pub fn redacted_order_summary(order: &crate::pages::StageOrderRequest) -> serde_json::Value {
    serde_json::json!({
        "security_code": order.security_code,
        "side": format!("{:?}", order.side).to_lowercase(),
        "price": order.price,
        "quantity": order.quantity
    })
}

pub fn redacted_cancellation_summary(
    target: &crate::pages::CancellationTarget,
) -> serde_json::Value {
    serde_json::json!({
        "contract_id": target.contract_id,
        "security_code": target.security_code,
        "security_name": target.security_name,
        "side": target.side,
        "price": target.price,
        "quantity": target.quantity
    })
}

#[derive(Clone, Debug)]
pub enum LivePayload {
    Order(crate::pages::StageOrderRequest),
    Cancellation(crate::pages::CancellationTarget),
}

pub fn fingerprint(
    kind: crate::storage::LiveOperationKind,
    payload: &LivePayload,
) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = match payload {
        LivePayload::Order(order) => serde_json::to_vec(&(kind, order))?,
        LivePayload::Cancellation(target) => serde_json::to_vec(&(kind, target))?,
    };
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub fn reconstruct_order(
    value: &serde_json::Value,
) -> anyhow::Result<crate::pages::StageOrderRequest> {
    use anyhow::Context;
    let security_code = value
        .get("security_code")
        .and_then(|v| v.as_str())
        .context("missing security_code")?
        .to_string();
    let side_str = value
        .get("side")
        .and_then(|v| v.as_str())
        .context("missing side")?;
    let side = match side_str {
        "buy" => crate::pages::OrderSide::Buy,
        "sell" => crate::pages::OrderSide::Sell,
        _ => anyhow::bail!("unknown side {side_str}"),
    };
    let price = value
        .get("price")
        .and_then(|v| v.as_str())
        .context("missing price")?
        .to_string();
    let quantity = value
        .get("quantity")
        .and_then(|v| v.as_u64())
        .context("missing quantity")?;
    Ok(crate::pages::StageOrderRequest {
        security_code,
        side,
        price,
        quantity,
    })
}

pub fn reconstruct_cancellation(
    value: &serde_json::Value,
) -> anyhow::Result<crate::pages::CancellationTarget> {
    use anyhow::Context;
    Ok(crate::pages::CancellationTarget {
        contract_id: value
            .get("contract_id")
            .and_then(|v| v.as_str())
            .context("missing contract_id")?
            .to_string(),
        security_code: value
            .get("security_code")
            .and_then(|v| v.as_str())
            .context("missing security_code")?
            .to_string(),
        security_name: value
            .get("security_name")
            .and_then(|v| v.as_str())
            .context("missing security_name")?
            .to_string(),
        side: value
            .get("side")
            .and_then(|v| v.as_str())
            .context("missing side")?
            .to_string(),
        price: value
            .get("price")
            .and_then(|v| v.as_str())
            .context("missing price")?
            .to_string(),
        quantity: value
            .get("quantity")
            .and_then(|v| v.as_u64())
            .context("missing quantity")?,
    })
}

#[cfg(test)]
mod tests {
    use super::{redacted_cancellation_summary, redacted_order_summary};
    use crate::pages::{CancellationTarget, OrderSide, StageOrderRequest};

    fn test_order() -> StageOrderRequest {
        StageOrderRequest {
            security_code: "600028".to_string(),
            side: OrderSide::Buy,
            price: "4.55".to_string(),
            quantity: 100,
        }
    }

    fn test_cancellation() -> CancellationTarget {
        CancellationTarget {
            contract_id: "3506784".to_string(),
            security_code: "600028".to_string(),
            security_name: "中国石化".to_string(),
            side: "买入".to_string(),
            price: "4.55".to_string(),
            quantity: 100,
        }
    }

    #[test]
    fn redacted_summaries_do_not_contain_secrets() {
        let order_summary = redacted_order_summary(&test_order());
        let cancel_summary = redacted_cancellation_summary(&test_cancellation());
        let order_str = order_summary.to_string();
        let cancel_str = cancel_summary.to_string();
        for needle in [
            "confirmation_token",
            "auth_token",
            "bearer",
            "Authorization",
            "STOCK_OPERATOR",
        ] {
            assert!(!order_str.to_lowercase().contains(&needle.to_lowercase()));
            assert!(!cancel_str.to_lowercase().contains(&needle.to_lowercase()));
        }
        assert!(order_str.contains("600028"));
        assert!(order_str.contains("4.55"));
        assert!(cancel_str.contains("3506784"));
    }
}
