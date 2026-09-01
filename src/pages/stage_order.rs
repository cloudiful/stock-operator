use std::{thread, time::Duration};

use anyhow::{Context, Result, bail};
use axuielement::AXUIElement;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    helpers::{collect_elements, element_geometry, read_string, read_text},
    reader::PageReader,
    stage_order_validation::{
        canonical_price, order_cents, order_value, parse_decimal, validate_request,
    },
    types::WorkspaceKind,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct StageOrderRequest {
    pub security_code: String,
    pub side: OrderSide,
    pub price: String,
    pub quantity: u64,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct StageOrderResult {
    pub status: String,
    pub request: StageOrderRequest,
    pub workspace: WorkspaceKind,
    pub order_value: String,
    pub verified_price: String,
    pub verified_quantity: String,
    pub warnings: Vec<String>,
}

impl PageReader {
    pub fn stage_order(&self, request: StageOrderRequest) -> Result<StageOrderResult> {
        validate_request(&request)?;
        self.focus_target_window()?;
        let form = self.trade_form()?;
        let expected_workspace = match request.side {
            OrderSide::Buy => WorkspaceKind::BuyOrder,
            OrderSide::Sell => WorkspaceKind::SellOrder,
        };
        if form.workspace != expected_workspace {
            bail!(
                "wrong trading workspace: expected {expected_workspace:?}, found {:?}",
                form.workspace
            );
        }
        self.select_trade_security(&request.security_code)?;
        let window = self.focus_target_window()?;
        let form = self.trade_form()?;
        if form.workspace != expected_workspace {
            bail!("trading workspace changed while selecting the requested security");
        }
        let initial_price = current_trade_price(&window)?;
        validate_trade_ocr(&self.ocr_visible_text()?, &request)?;
        let focused_window = self.focus_target_window()?;
        if !same_geometry(element_geometry(&window), element_geometry(&focused_window)) {
            bail!("target window geometry changed while validating the staged order");
        }
        let current_form = self.trade_form()?;
        if current_form.workspace != expected_workspace {
            bail!("trading workspace changed while validating the staged order");
        }
        thread::sleep(Duration::from_millis(100));
        validate_trade_ocr(&self.ocr_visible_text()?, &request)?;
        let (price_field, quantity_field) = trade_fields(&focused_window)?;
        if read_text(&price_field).as_deref() != Some(&initial_price) {
            bail!("client-derived trade price changed while validating the staged order");
        }
        let price = canonical_price(&request.price)?;
        let quantity = request.quantity.to_string();
        validate_price_band(&price_field, &request.price)?;
        stage_fields(&price_field, &quantity_field, &price, &quantity)?;
        let final_window = self.focus_target_window()?;
        let (final_price, final_quantity) = trade_fields(&final_window)?;
        if read_text(&final_price).as_deref() != Some(&price)
            || read_text(&final_quantity).as_deref() != Some(&quantity)
        {
            bail!("trade form changed after staging; staged values are not verified");
        }
        let order_value = order_value(&request)?;
        Ok(StageOrderResult {
            status: "staged_not_submitted".to_string(),
            request,
            workspace: form.workspace,
            order_value,
            verified_price: price,
            verified_quantity: quantity,
            warnings: vec![
                "price and quantity were written and read back through Accessibility".to_string(),
                "security code was selected and verified before staging price and quantity"
                    .to_string(),
                "confirmation, keyboard events, and order submission are disabled".to_string(),
            ],
        })
    }
}

fn validate_trade_ocr(ocr: &super::ocr::OcrSnapshot, request: &StageOrderRequest) -> Result<()> {
    if !selected_security_matches(ocr, &request.security_code) {
        bail!(
            "current trade form security does not uniquely match {}",
            request.security_code
        );
    }
    match request.side {
        OrderSide::Buy => validate_available_funds(ocr, request),
        OrderSide::Sell => validate_available_shares(ocr, request),
    }
}

fn same_geometry(left: Option<(f64, f64, f64, f64)>, right: Option<(f64, f64, f64, f64)>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    [
        (left.0 - right.0).abs(),
        (left.1 - right.1).abs(),
        (left.2 - right.2).abs(),
        (left.3 - right.3).abs(),
    ]
    .into_iter()
    .all(|difference| difference <= 1.0)
}

fn selected_security_matches(ocr: &super::ocr::OcrSnapshot, code: &str) -> bool {
    ocr.observations
        .iter()
        .filter(|observation| observation.x < 0.18 && observation.y > 0.84 && observation.y < 0.90)
        .filter(|observation| observation.text.split_whitespace().any(|part| part == code))
        .count()
        == 1
}

fn validate_available_funds(
    ocr: &super::ocr::OcrSnapshot,
    request: &StageOrderRequest,
) -> Result<()> {
    let values = ocr
        .observations
        .iter()
        .filter(|observation| observation.x > 0.06 && observation.x < 0.10)
        .filter(|observation| observation.y > 0.72 && observation.y < 0.75)
        .filter_map(|observation| parse_decimal(&observation.text).ok())
        .collect::<Vec<_>>();
    if values.len() != 1 {
        bail!("available funds could not be identified uniquely from the trade form");
    }
    if order_cents(request)? > values[0] {
        bail!("order value exceeds available funds shown in the trade form");
    }
    Ok(())
}

fn validate_available_shares(
    ocr: &super::ocr::OcrSnapshot,
    request: &StageOrderRequest,
) -> Result<()> {
    let values = ocr
        .observations
        .iter()
        .filter(|observation| observation.x > 0.06 && observation.x < 0.10)
        .filter(|observation| observation.y > 0.72 && observation.y < 0.75)
        .filter_map(|observation| parse_decimal(&observation.text).ok())
        .collect::<Vec<_>>();
    if values.len() != 1 {
        bail!("maximum sellable shares could not be identified uniquely from the trade form");
    }
    let requested = i128::from(request.quantity);
    if requested > values[0] {
        bail!("sell quantity exceeds maximum sellable shares shown in the trade form");
    }
    Ok(())
}

fn trade_fields(window: &AXUIElement) -> Result<(AXUIElement, AXUIElement)> {
    let mut fields = collect_elements(window, 1_000)
        .into_iter()
        .filter(|element| read_string(element, "AXRole").as_deref() == Some("AXTextField"))
        .filter(|element| {
            element
                .action_names()
                .unwrap_or_default()
                .iter()
                .any(|action| action == "AXConfirm")
        })
        .filter_map(|element| element_geometry(&element).map(|geometry| (element, geometry)))
        .collect::<Vec<_>>();
    if fields.len() != 2 {
        bail!(
            "expected exactly two stable trade text fields, found {}",
            fields.len()
        );
    }
    fields.sort_by(|left, right| left.1.1.total_cmp(&right.1.1));
    let (price, price_geometry) = fields.remove(0);
    let (quantity, quantity_geometry) = fields.remove(0);
    if (price_geometry.0 - quantity_geometry.0).abs() > 4.0
        || (price_geometry.2 - quantity_geometry.2).abs() > 4.0
        || quantity_geometry.1 <= price_geometry.1
    {
        bail!("trade text field geometry does not match the expected price/quantity layout");
    }
    let price_value = read_text(&price).context("price field value is unavailable")?;
    let quantity_value = read_text(&quantity).unwrap_or_default();
    if !price_value.contains('.')
        || parse_decimal(&price_value).is_err()
        || (!quantity_value.is_empty() && !quantity_value.bytes().all(|byte| byte.is_ascii_digit()))
    {
        bail!("trade text field values do not match the expected price/quantity shape");
    }
    Ok((price, quantity))
}

fn current_trade_price(window: &AXUIElement) -> Result<String> {
    let (price, _) = trade_fields(window)?;
    read_text(&price).context("client-derived trade price is unavailable")
}

fn stage_fields(
    price_field: &AXUIElement,
    quantity_field: &AXUIElement,
    price: &str,
    quantity: &str,
) -> Result<()> {
    let original_price = read_text(price_field).context("price field value is unavailable")?;
    let original_quantity = read_text(quantity_field).unwrap_or_default();
    if let Err(error) = set_and_verify(price_field, price, "price")
        .and_then(|()| set_and_verify(quantity_field, quantity, "quantity"))
    {
        let price_restored = set_and_verify(price_field, &original_price, "price rollback");
        let quantity_restored =
            set_and_verify(quantity_field, &original_quantity, "quantity rollback");
        if price_restored.is_err() || quantity_restored.is_err() {
            bail!(
                "staging failed and the original form values could not be fully restored: {error}"
            );
        }
        return Err(error);
    }
    Ok(())
}

fn set_and_verify(field: &AXUIElement, value: &str, name: &str) -> Result<()> {
    field
        .set_string_attribute("AXValue", value)
        .map_err(|error| anyhow::anyhow!("failed to write {name} field: {error:?}"))?;
    for _ in 0..5 {
        if read_text(field).as_deref() == Some(value) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    bail!("{name} field did not read back the staged value")
}

fn validate_price_band(field: &AXUIElement, requested: &str) -> Result<()> {
    let current = read_text(field).context("current price field value is unavailable")?;
    let current = parse_decimal(&current)?;
    let requested = parse_decimal(requested)?;
    let difference = (requested - current).abs();
    let Some(scaled_difference) = difference.checked_mul(100) else {
        bail!("requested price difference overflow");
    };
    let Some(limit) = current.checked_mul(10) else {
        bail!("current price overflow");
    };
    if scaled_difference > limit {
        bail!("requested price differs from the current form price by more than 10 percent");
    }
    Ok(())
}
