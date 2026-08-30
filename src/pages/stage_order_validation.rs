use anyhow::{Result, bail};

use super::stage_order::{OrderSide, StageOrderRequest};

pub(crate) fn validate_request(request: &StageOrderRequest) -> Result<()> {
    let code = request.security_code.trim();
    if !(code.len() == 5 || code.len() == 6) || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("security_code must contain exactly 5 or 6 ASCII digits");
    }
    if request.quantity == 0 || request.quantity > 1_000_000 {
        bail!("quantity must be between 1 and 1,000,000");
    }
    if request.side == OrderSide::Buy && code.len() == 6 && request.quantity % 100 != 0 {
        bail!("A-share buy quantity must be a multiple of 100 shares");
    }
    let price = parse_decimal(&request.price)?;
    if price <= 0 {
        bail!("price must be positive");
    }
    if order_cents(request)? > 500_000 {
        bail!("order value exceeds the local 5,000 RMB limit");
    }
    Ok(())
}

pub(crate) fn order_cents(request: &StageOrderRequest) -> Result<i128> {
    parse_decimal(&request.price)?
        .checked_mul(i128::from(request.quantity))
        .ok_or_else(|| anyhow::anyhow!("order value overflow"))
}

pub(super) fn order_value(request: &StageOrderRequest) -> Result<String> {
    Ok(format!("{:.2}", order_cents(request)? as f64 / 100.0))
}

pub(super) fn canonical_price(value: &str) -> Result<String> {
    let cents = parse_decimal(value)?;
    Ok(format!("{}.{:02}", cents / 100, cents % 100))
}

pub(crate) fn parse_decimal(value: &str) -> Result<i128> {
    let value = value.trim().replace(',', "");
    let (whole, fraction) = value.split_once('.').unwrap_or((&value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("price must be a positive decimal with at most two fractional digits");
    }
    let cents = whole
        .parse::<i128>()?
        .checked_mul(100)
        .ok_or_else(|| anyhow::anyhow!("price overflow"))?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i128>()? * 10,
        _ => fraction.parse::<i128>()?,
    };
    cents
        .checked_add(fraction)
        .ok_or_else(|| anyhow::anyhow!("price overflow"))
}

#[cfg(test)]
mod tests {
    use super::{order_value, validate_request};
    use crate::pages::stage_order::{OrderSide, StageOrderRequest};

    fn request(price: &str, quantity: u64) -> StageOrderRequest {
        StageOrderRequest {
            security_code: "600028".to_string(),
            side: OrderSide::Buy,
            price: price.to_string(),
            quantity,
        }
    }

    #[test]
    fn validates_and_formats_order_value() {
        let request = request("5.06", 100);
        validate_request(&request).unwrap();
        assert_eq!(order_value(&request).unwrap(), "506.00");
    }

    #[test]
    fn rejects_invalid_code_quantity_lot_and_limit() {
        let mut invalid = request("5.06", 100);
        invalid.security_code = "6002".to_string();
        assert!(validate_request(&invalid).is_err());
        assert!(validate_request(&request("5.06", 0)).is_err());
        assert!(validate_request(&request("5.06", 99)).is_err());
        assert!(validate_request(&request("500.00", 100)).is_err());
    }

    #[test]
    fn permits_odd_lot_sell_for_full_position_exit() {
        let mut request = request("5.06", 99);
        request.side = OrderSide::Sell;
        validate_request(&request).unwrap();
    }
}
