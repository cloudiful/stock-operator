use std::{thread, time::Duration};

use anyhow::{Context, Result, bail};
use axuielement::AXUIElement;
use schemars::JsonSchema;
use serde::Serialize;

use super::{
    helpers::{collect_elements, element_geometry, read_string, read_text},
    reader::PageReader,
};

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct SecuritySelectionResult {
    pub security_code: String,
    pub selected_text: String,
    pub derived_price: String,
}

impl PageReader {
    pub fn select_trade_security(&self, security_code: &str) -> Result<SecuritySelectionResult> {
        validate_security_code(security_code)?;
        self.focus_target_window()?;
        if !matches!(
            self.view()?.workspace,
            super::types::WorkspaceKind::BuyOrder | super::types::WorkspaceKind::SellOrder
        ) {
            bail!("security selection requires a verified buy or sell workspace");
        }

        let initial_ocr = self.ocr_visible_text()?;
        if selected_security_text(&initial_ocr, security_code).is_none()
            && security_code_field(&self.focus_target_window()?).is_err()
        {
            let selected = selected_security_label(&initial_ocr)
                .context("current selected security could not be identified uniquely")?;
            super::ocr_navigation::click_selected_security(self, &selected)?;
            thread::sleep(Duration::from_millis(150));
        }

        if let Some(selected_text) =
            selected_security_text(&self.ocr_visible_text()?, security_code)
            && security_code_field(&self.focus_target_window()?).is_err()
        {
            let derived_price = stable_derived_price(self)?;
            return Ok(SecuritySelectionResult {
                security_code: security_code.to_string(),
                selected_text,
                derived_price,
            });
        }

        let window = self.focus_target_window()?;
        let field = security_code_field(&window)?;
        field
            .set_string_attribute("AXValue", security_code)
            .map_err(|error| anyhow::anyhow!("failed to write security code: {error:?}"))?;
        if read_text(&field).as_deref() != Some(security_code) {
            bail!("security code field did not read back the requested code");
        }
        field
            .perform_action("AXConfirm")
            .map_err(|error| anyhow::anyhow!("failed to confirm security code field: {error:?}"))?;

        for _ in 0..20 {
            thread::sleep(Duration::from_millis(100));
            let ocr = self.ocr_visible_text()?;
            if let Some(selected_text) = selected_security_text(&ocr, security_code) {
                if security_code_field(&self.focus_target_window()?).is_ok() {
                    continue;
                }
                let derived_price = stable_derived_price(self)?;
                return Ok(SecuritySelectionResult {
                    security_code: security_code.to_string(),
                    selected_text,
                    derived_price,
                });
            }
        }
        bail!("client did not select the requested security after field confirmation")
    }
}

fn validate_security_code(code: &str) -> Result<()> {
    if !(code.len() == 5 || code.len() == 6) || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("security code must contain exactly 5 or 6 ASCII digits");
    }
    Ok(())
}

pub(super) fn security_code_field(window: &AXUIElement) -> Result<AXUIElement> {
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
        .filter(|element| {
            element_geometry(element)
                .is_some_and(|(_, _, width, height)| width >= 180.0 && height >= 28.0)
        })
        .collect::<Vec<_>>();
    if fields.len() != 1 {
        bail!(
            "expected exactly one stable security-code field, found {}",
            fields.len()
        );
    }
    Ok(fields.remove(0))
}

fn selected_security_label(ocr: &super::ocr::OcrSnapshot) -> Option<String> {
    let mut matches = ocr
        .observations
        .iter()
        .filter(|observation| observation.x < 0.16 && (0.84..0.90).contains(&observation.y))
        .filter(|observation| {
            let mut parts = observation.text.split_whitespace();
            parts.next().is_some_and(|code| {
                (code.len() == 5 || code.len() == 6)
                    && code.bytes().all(|byte| byte.is_ascii_digit())
            }) && parts.next().is_some()
        })
        .map(|observation| observation.text.trim().to_string())
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches.remove(0))
}

fn selected_security_text(ocr: &super::ocr::OcrSnapshot, security_code: &str) -> Option<String> {
    selected_security_label(ocr).filter(|text| {
        text.split_whitespace()
            .next()
            .is_some_and(|code| code == security_code)
    })
}

fn stable_derived_price(reader: &PageReader) -> Result<String> {
    let mut previous = None;
    for _ in 0..10 {
        let window = reader.focus_target_window()?;
        let mut fields = collect_elements(&window, 1_000)
            .into_iter()
            .filter(|element| read_string(element, "AXRole").as_deref() == Some("AXTextField"))
            .filter_map(|element| {
                let geometry = element_geometry(&element)?;
                (geometry.2 < 180.0).then(|| (element, geometry))
            })
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.1.1.total_cmp(&right.1.1));
        let price = fields
            .first()
            .and_then(|(field, _)| read_text(field))
            .filter(|value| value.contains('.'));
        if price.is_some() && price == previous {
            return price.context("derived price is unavailable");
        }
        previous = price;
        thread::sleep(Duration::from_millis(100));
    }
    bail!("client-derived price did not become stable")
}

#[cfg(test)]
mod tests {
    use super::validate_security_code;

    #[test]
    fn validates_supported_security_codes() {
        validate_security_code("600028").unwrap();
        validate_security_code("00700").unwrap();
        assert!(validate_security_code("6002").is_err());
        assert!(validate_security_code("60002A").is_err());
    }
}
