use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    helpers::{collect_elements, read_string},
    reader::PageReader,
    types::PanelKind,
};

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct CancelNavigationResult {
    pub status: String,
    pub panel: PanelKind,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct CancellationTarget {
    pub contract_id: String,
    pub security_code: String,
    pub security_name: String,
    pub side: String,
    pub price: String,
    pub quantity: u64,
}

impl PageReader {
    pub fn navigate_to_cancellations(&self) -> Result<CancelNavigationResult> {
        super::ocr_navigation::click_exact_label(self, "撤单", 0.93..0.97)?;
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let view = self.view()?;
            if view.panel == PanelKind::Cancellations {
                return Ok(CancelNavigationResult {
                    status: "cancellations_opened".to_string(),
                    panel: view.panel,
                });
            }
        }
        bail!("cancellation page fingerprint was not verified after navigation")
    }

    pub fn open_cancel_confirmation(&self, target: &CancellationTarget) -> Result<()> {
        let contract_id = target.contract_id.as_str();
        if contract_id.is_empty() || !contract_id.bytes().all(|byte| byte.is_ascii_digit()) {
            bail!("contract id must contain only ASCII digits");
        }
        let ocr = self.ocr_visible_text()?;
        let quantity = target.quantity.to_string();
        for expected in [
            contract_id,
            target.security_code.as_str(),
            target.security_name.as_str(),
            target.side.as_str(),
            "已申报",
            quantity.as_str(),
        ] {
            if !ocr.observations.iter().any(|observation| {
                observation.confidence >= 0.9 && observation.text.trim() == expected
            }) {
                bail!("cancellation row does not contain expected value: {expected}");
            }
        }
        let contract_y = ocr
            .observations
            .iter()
            .find(|observation| observation.text.trim() == contract_id)
            .map(|observation| observation.y)
            .context("contract row geometry is unavailable")?;
        if !ocr.observations.iter().any(|observation| {
            observation.confidence >= 0.9
                && observation.text.trim().starts_with(&target.price)
                && (observation.y - contract_y).abs() <= 0.01
        }) {
            bail!(
                "cancellation row does not contain expected price {}",
                target.price
            );
        }
        let row_matches = [
            target.security_code.as_str(),
            target.security_name.as_str(),
            target.side.as_str(),
            "已申报",
            quantity.as_str(),
        ]
        .iter()
        .all(|expected| {
            ocr.observations.iter().any(|observation| {
                observation.confidence >= 0.9
                    && observation.text.trim() == *expected
                    && (observation.y - contract_y).abs() <= 0.01
            })
        });
        if !row_matches {
            bail!("cancellation row fields are not aligned with contract {contract_id}");
        }
        super::ocr_navigation::double_click_exact_label(self, contract_id, 0.30..0.90)?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        let window = self.focus_target_window()?;
        if read_string(&window, "AXSubrole").as_deref() != Some("AXDialog") {
            bail!("double-click did not open a cancellation dialog");
        }
        let text = collect_elements(&window, 50)
            .into_iter()
            .filter_map(|element| super::helpers::read_text(&element))
            .collect::<Vec<_>>()
            .join("\n");
        if text.contains("请选择您要撤销的交易单") {
            bail!("double-click did not select the target cancellation row");
        }
        if !text.contains("撤单确认") {
            bail!("double-click opened an unexpected dialog");
        }
        Ok(())
    }

    pub fn close_cancel_selection_warning(&self) -> Result<()> {
        let window = self.focus_target_window()?;
        if read_string(&window, "AXSubrole").as_deref() != Some("AXDialog") {
            bail!("the focused target window is not a cancellation warning dialog");
        }
        let elements = collect_elements(&window, 50);
        let text = elements
            .iter()
            .filter_map(super::helpers::read_text)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.contains("撤单确认") || !text.contains("请选择您要撤销的交易单") {
            bail!("focused dialog is not the expected cancellation selection warning");
        }
        let buttons = elements
            .iter()
            .filter(|element| read_string(element, "AXRole").as_deref() == Some("AXButton"))
            .filter(|element| read_string(element, "AXTitle").as_deref() == Some("确定"))
            .collect::<Vec<_>>();
        if buttons.len() != 1 {
            bail!("expected exactly one warning dismissal button");
        }
        buttons[0].perform_action("AXPress").map_err(|error| {
            anyhow::anyhow!("failed to dismiss cancellation warning: {error:?}")
        })?;
        Ok(())
    }

    pub fn confirm_cancel_order(&self, target: &CancellationTarget) -> Result<()> {
        let window = self.focus_target_window()?;
        if read_string(&window, "AXSubrole").as_deref() != Some("AXDialog") {
            bail!("the focused target window is not a cancellation confirmation dialog");
        }
        let elements = collect_elements(&window, 50);
        let text = elements
            .iter()
            .filter_map(super::helpers::read_text)
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "撤单确认",
            "操作类别:   股票撤单",
            &format!("买卖方向:   {}", target.side),
            &format!("证券代码:   {}", target.security_code),
            &format!("证券名称:   {}", target.security_name),
        ] {
            if !text.contains(expected) {
                bail!("cancellation confirmation does not match: {expected}");
            }
        }
        if !text.contains(&format!("合同编号:   {}", target.contract_id)) {
            bail!("cancellation confirmation contract id does not match");
        }
        let buttons = elements
            .iter()
            .filter(|element| read_string(element, "AXRole").as_deref() == Some("AXButton"))
            .filter(|element| read_string(element, "AXTitle").as_deref() == Some("确定"))
            .filter(|element| {
                read_string(element, "AXIdentifier").as_deref() == Some("action-button-1")
            })
            .collect::<Vec<_>>();
        if buttons.len() != 1 {
            bail!("expected exactly one verified cancellation confirmation button");
        }
        buttons[0]
            .perform_action("AXPress")
            .map_err(|error| anyhow::anyhow!("failed to confirm cancellation: {error:?}"))?;
        Ok(())
    }

    pub fn close_cancel_submitted_notice(&self) -> Result<()> {
        let window = self.focus_target_window()?;
        let elements = collect_elements(&window, 50);
        let text = elements
            .iter()
            .filter_map(super::helpers::read_text)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.contains("撤单已提交") {
            bail!("focused dialog is not the expected cancellation result");
        }
        let buttons = elements
            .iter()
            .filter(|element| read_string(element, "AXRole").as_deref() == Some("AXButton"))
            .filter(|element| read_string(element, "AXTitle").as_deref() == Some("确定"))
            .collect::<Vec<_>>();
        if buttons.len() != 1 {
            bail!("expected exactly one cancellation result button");
        }
        buttons[0]
            .perform_action("AXPress")
            .map_err(|error| anyhow::anyhow!("failed to close cancellation result: {error:?}"))?;
        Ok(())
    }

    pub fn cancel_cancellation_confirmation(&self, target: &CancellationTarget) -> Result<()> {
        let window = self.focus_target_window()?;
        let elements = collect_elements(&window, 50);
        let text = elements
            .iter()
            .filter_map(super::helpers::read_text)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.contains("撤单确认")
            || !text.contains(&target.contract_id)
            || !text.contains(&target.security_code)
        {
            bail!("cancellation dialog does not match abort request");
        }
        let buttons = elements
            .iter()
            .filter(|element| read_string(element, "AXRole").as_deref() == Some("AXButton"))
            .filter(|element| read_string(element, "AXTitle").as_deref() == Some("取消"))
            .collect::<Vec<_>>();
        if buttons.len() != 1 {
            bail!("expected exactly one cancellation-dialog cancel button");
        }
        buttons[0]
            .perform_action("AXPress")
            .map_err(|error| anyhow::anyhow!("failed to abort cancellation dialog: {error:?}"))
    }
}
