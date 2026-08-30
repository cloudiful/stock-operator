use anyhow::Result;

use super::{
    ocr::{OcrObservation, OcrSnapshot},
    stage_order::{OrderSide, StageOrderRequest},
    stage_order_validation::{order_cents, parse_decimal, validate_request},
    trade_preflight::{
        BrokerFormState, BrokerQuote, BrokerTradeConstraints, PreflightDecision, QuoteComparison,
        ReferenceQuote, SecurityIdentity,
    },
    types::{DataQuality, WorkspaceKind},
};

pub(super) fn evaluate_order(
    order: Option<&StageOrderRequest>,
    workspace: WorkspaceKind,
    security: Option<&SecurityIdentity>,
    form: &BrokerFormState,
    quote: &BrokerQuote,
    constraints: &BrokerTradeConstraints,
    comparison: Option<&QuoteComparison>,
    warnings: &mut Vec<String>,
) -> Result<PreflightDecision> {
    let Some(order) = order else {
        return Ok(PreflightDecision::ObservedOnly);
    };
    if let Err(error) = validate_request(order) {
        warnings.push(error.to_string());
        return Ok(PreflightDecision::Rejected);
    }
    let expected_workspace = match order.side {
        OrderSide::Buy => WorkspaceKind::BuyOrder,
        OrderSide::Sell => WorkspaceKind::SellOrder,
    };
    if workspace != expected_workspace {
        warnings.push("current workspace does not match the requested order side".to_string());
        return Ok(PreflightDecision::Rejected);
    }
    if security.is_none_or(|value| value.code != order.security_code) {
        warnings.push("current broker security does not match the requested order".to_string());
        return Ok(PreflightDecision::Rejected);
    }
    let requested_price = parse_decimal(&order.price)?;
    if let Some(limit) = quote
        .down_limit
        .as_deref()
        .and_then(|value| parse_decimal(value).ok())
        && requested_price < limit
    {
        warnings.push("requested price is below the broker down-limit".to_string());
        return Ok(PreflightDecision::Rejected);
    }
    if let Some(limit) = quote
        .up_limit
        .as_deref()
        .and_then(|value| parse_decimal(value).ok())
        && requested_price > limit
    {
        warnings.push("requested price is above the broker up-limit".to_string());
        return Ok(PreflightDecision::Rejected);
    }
    let mut needs_review = match comparison {
        Some(value) if value.status == "matched" => false,
        Some(value) if value.status == "invalid_reference" => {
            warnings.push("backend reference quote is invalid".to_string());
            true
        }
        Some(_) => {
            warnings.push("backend reference quote differs from the broker quote".to_string());
            true
        }
        None => {
            warnings.push("backend reference quote is unavailable".to_string());
            true
        }
    };
    if form.control_count < 2 || form.price.is_none() || form.quantity.is_none() {
        warnings.push("broker trade form fields are incomplete".to_string());
        needs_review = true;
    }
    for (value, message, numeric) in [
        (
            &quote.latest_price,
            "broker latest price is unavailable or invalid",
            true,
        ),
        (
            &quote.up_limit,
            "broker up-limit is unavailable or invalid",
            true,
        ),
        (
            &quote.down_limit,
            "broker down-limit is unavailable or invalid",
            true,
        ),
        (
            &quote.trade_status,
            "broker trade status is unavailable",
            false,
        ),
    ] {
        if value.is_none()
            || (numeric
                && value
                    .as_deref()
                    .is_some_and(|value| parse_decimal(value).is_err()))
        {
            warnings.push(message.to_string());
            needs_review = true;
        }
    }
    if quote.trade_status.as_deref().is_some_and(|status| {
        ["停牌", "休市", "非交易", "不可交易", "暂停"]
            .iter()
            .any(|token| status.contains(token))
    }) {
        return Ok(rejected(
            warnings,
            "broker trade status does not permit trading",
        ));
    }
    match order.side {
        OrderSide::Buy => {
            if let Some(funds) = constraints
                .available_funds
                .as_deref()
                .and_then(|value| parse_decimal(value).ok())
            {
                if order_cents(order)? > funds {
                    return Ok(rejected(
                        warnings,
                        "order value exceeds broker available funds",
                    ));
                }
            } else {
                needs_review = true;
            }
            if let Some(maximum) = constraints.max_buy_quantity {
                if order.quantity > maximum {
                    return Ok(rejected(
                        warnings,
                        "order quantity exceeds broker maximum buy quantity",
                    ));
                }
            } else {
                needs_review = true;
            }
        }
        OrderSide::Sell => {
            if let Some(maximum) = constraints.max_sell_quantity {
                if order.quantity > maximum {
                    return Ok(rejected(
                        warnings,
                        "order quantity exceeds broker maximum sell quantity",
                    ));
                }
            } else {
                needs_review = true;
            }
        }
    }
    Ok(if needs_review {
        PreflightDecision::NeedsReview
    } else {
        PreflightDecision::Ready
    })
}

fn rejected(warnings: &mut Vec<String>, message: &str) -> PreflightDecision {
    warnings.push(message.to_string());
    PreflightDecision::Rejected
}

pub(super) fn selected_security(ocr: &OcrSnapshot) -> Option<SecurityIdentity> {
    let mut matches = ocr.observations.iter().filter_map(|observation| {
        let mut parts = observation.text.split_whitespace();
        let code = parts.next()?;
        let name = parts.collect::<Vec<_>>().join(" ");
        (observation.x < 0.2
            && (0.84..0.90).contains(&observation.y)
            && (code.len() == 5 || code.len() == 6)
            && code.bytes().all(|byte| byte.is_ascii_digit())
            && !name.is_empty())
        .then_some(SecurityIdentity {
            code: code.to_string(),
            name,
        })
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

pub(super) fn value_near_label(ocr: &OcrSnapshot, labels: &[&str]) -> Option<String> {
    observations_near_label(ocr, labels).find_map(|observation| numeric_value(&observation.text))
}

pub(super) fn integer_near_label(ocr: &OcrSnapshot, labels: &[&str]) -> Option<u64> {
    value_near_label(ocr, labels).and_then(|value| {
        value.parse().ok().or_else(|| {
            let cents = parse_decimal(&value).ok()?;
            (cents % 100 == 0).then_some((cents / 100) as u64)
        })
    })
}

pub(super) fn text_near_label(ocr: &OcrSnapshot, labels: &[&str]) -> Option<String> {
    observations_near_label(ocr, labels).find_map(|observation| {
        let value = observation.text.trim();
        (!labels.iter().any(|label| value == *label)).then_some(value.to_string())
    })
}

fn observations_near_label<'a>(
    ocr: &'a OcrSnapshot,
    labels: &[&str],
) -> impl Iterator<Item = &'a OcrObservation> {
    let labels = ocr
        .observations
        .iter()
        .filter(|observation| labels.iter().any(|label| observation.text.contains(label)))
        .collect::<Vec<_>>();
    ocr.observations.iter().filter(move |candidate| {
        labels.iter().any(|label| {
            candidate.x >= label.x
                && candidate.x - label.x < 0.18
                && (candidate.y - label.y).abs() <= 0.025
                && candidate.text != label.text
        })
    })
}

fn numeric_value(text: &str) -> Option<String> {
    let mut matches = text.split_whitespace().filter_map(|token| {
        let normalized = token.replace(',', "").replace('．', ".");
        let value = normalized.parse::<f64>().ok()?;
        value.is_finite().then_some(normalized)
    });
    let value = matches.next()?;
    matches.next().is_none().then_some(value)
}

pub(super) fn compare_quote(
    reference: &ReferenceQuote,
    broker_price: Option<&str>,
) -> QuoteComparison {
    let Some(reference_value) = parse_decimal(&reference.latest_price)
        .ok()
        .map(|value| value as f64 / 100.0)
    else {
        return QuoteComparison {
            reference_source: reference.source.clone(),
            reference_price: reference.latest_price.clone(),
            broker_price: broker_price.map(str::to_string),
            absolute_delta: None,
            percentage_delta: None,
            status: "invalid_reference".to_string(),
        };
    };
    let broker_value = broker_price
        .and_then(|value| parse_decimal(value).ok())
        .map(|value| value as f64 / 100.0);
    let (absolute_delta, percentage_delta, status) = match broker_value {
        Some(value) if reference_value != 0.0 => {
            let delta = (value - reference_value).abs();
            let percentage = delta / reference_value * 100.0;
            (
                Some(format!("{delta:.4}")),
                Some(format!("{percentage:.4}")),
                if delta <= 0.02 {
                    "matched"
                } else {
                    "different"
                },
            )
        }
        Some(value) => (
            Some(format!("{:.4}", (value - reference_value).abs())),
            None,
            "different",
        ),
        None => (None, None, "unavailable"),
    };
    QuoteComparison {
        reference_source: reference.source.clone(),
        reference_price: reference.latest_price.clone(),
        broker_price: broker_price.map(str::to_string),
        absolute_delta,
        percentage_delta,
        status: status.to_string(),
    }
}

pub(super) fn quality(
    security: &Option<SecurityIdentity>,
    quote: &BrokerQuote,
    constraints: &BrokerTradeConstraints,
    warnings: &mut Vec<String>,
) -> DataQuality {
    let present = [
        security.is_some(),
        quote.latest_price.is_some(),
        quote.pre_close.is_some(),
        quote.up_limit.is_some(),
        quote.down_limit.is_some(),
        quote.buy_ceiling.is_some(),
        quote.sell_floor.is_some(),
        quote.bid_price_1.is_some(),
        quote.ask_price_1.is_some(),
        quote.trade_status.is_some(),
        constraints.available_funds.is_some(),
        constraints.max_buy_quantity.is_some(),
        constraints.max_sell_quantity.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if present == 13 {
        DataQuality::Exact
    } else if present > 0 {
        warnings.push("one or more broker preflight fields were unavailable".to_string());
        DataQuality::Partial
    } else {
        DataQuality::Unavailable
    }
}
