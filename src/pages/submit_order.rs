use anyhow::{Result, bail};
use schemars::JsonSchema;
use serde::Serialize;

use super::{
    helpers::{collect_elements, read_string, read_text},
    ocr::OcrObservation,
    reader::PageReader,
    stage_order::{OrderSide, StageOrderRequest},
    stage_order_validation::order_cents,
    types::ViewDescriptor,
};

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct SubmitOrderResult {
    pub status: String,
    pub request: StageOrderRequest,
    pub after: ViewDescriptor,
    pub visible_prompts: Vec<String>,
}

impl PageReader {
    pub fn open_order_confirmation(&self, request: StageOrderRequest) -> Result<SubmitOrderResult> {
        if request.side != OrderSide::Buy {
            bail!("live submission testing currently supports buy orders only");
        }
        if order_cents(&request)? > 50_000 {
            bail!("live submission test exceeds the 500 RMB hard limit");
        }
        self.stage_order(request.clone())?;
        super::ocr_navigation::click_exact_label(self, "买入下单", 0.58..0.65)?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        let ocr = self.ocr_visible_text()?;
        let visible_prompts = ocr
            .observations
            .iter()
            .filter(|observation| prompt_observation(observation))
            .map(|observation| observation.text.clone())
            .collect::<Vec<_>>();
        Ok(SubmitOrderResult {
            status: "confirmation_opened_or_order_submitted".to_string(),
            request,
            after: self.view()?,
            visible_prompts,
        })
    }

    pub fn confirm_open_order(&self, request: StageOrderRequest) -> Result<SubmitOrderResult> {
        if request.side != OrderSide::Buy || order_cents(&request)? > 50_000 {
            bail!("live confirmation is limited to buy orders at or below 500 RMB");
        }
        let window = self.focus_target_window()?;
        if read_string(&window, "AXSubrole").as_deref() != Some("AXDialog") {
            bail!("the focused target window is not an order confirmation dialog");
        }
        let elements = collect_elements(&window, 100);
        let text = elements
            .iter()
            .filter_map(read_text)
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "交易确认".to_string(),
            "操作类别:   买入".to_string(),
            format!("股票代码:   {}", request.security_code),
            format!("委托价格:   {}", request.price),
            format!("委托数量:   {}股", request.quantity),
        ] {
            if !text.contains(&expected) {
                bail!("order confirmation dialog does not match expected field: {expected}");
            }
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
            bail!("expected exactly one verified confirmation button");
        }
        buttons[0]
            .perform_action("AXPress")
            .map_err(|error| anyhow::anyhow!("failed to confirm verified order: {error:?}"))?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        let after_ocr = self.ocr_visible_text()?;
        Ok(SubmitOrderResult {
            status: "verified_confirmation_pressed".to_string(),
            request,
            after: self.view()?,
            visible_prompts: after_ocr
                .observations
                .iter()
                .filter(|observation| prompt_observation(observation))
                .map(|observation| observation.text.clone())
                .collect(),
        })
    }

    pub fn close_submitted_notice(&self, contract_id: &str) -> Result<()> {
        if contract_id.is_empty() || !contract_id.bytes().all(|byte| byte.is_ascii_digit()) {
            bail!("contract id must contain only ASCII digits");
        }
        let window = self.focus_target_window()?;
        if read_string(&window, "AXSubrole").as_deref() != Some("AXDialog") {
            bail!("the focused target window is not a submission result dialog");
        }
        let elements = collect_elements(&window, 50);
        let text = elements
            .iter()
            .filter_map(read_text)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.contains("委托已提交") || !text.contains(contract_id) {
            bail!("submission result does not match contract id {contract_id}");
        }
        let buttons = elements
            .iter()
            .filter(|element| read_string(element, "AXRole").as_deref() == Some("AXButton"))
            .filter(|element| read_string(element, "AXTitle").as_deref() == Some("确定"))
            .collect::<Vec<_>>();
        if buttons.len() != 1 {
            bail!("expected exactly one button on the submission result dialog");
        }
        buttons[0]
            .perform_action("AXPress")
            .map_err(|error| anyhow::anyhow!("failed to close submission result: {error:?}"))?;
        Ok(())
    }

    pub fn cancel_order_confirmation(&self, request: &StageOrderRequest) -> Result<()> {
        let window = self.focus_target_window()?;
        let elements = collect_elements(&window, 100);
        let text = elements
            .iter()
            .filter_map(read_text)
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "交易确认".to_string(),
            format!("股票代码:   {}", request.security_code),
            format!("委托价格:   {}", request.price),
            format!("委托数量:   {}股", request.quantity),
        ] {
            if !text.contains(&expected) {
                bail!("order dialog does not match abort request: {expected}");
            }
        }
        press_unique_cancel(&elements)
    }
}

fn press_unique_cancel(elements: &[axuielement::AXUIElement]) -> Result<()> {
    let buttons = elements
        .iter()
        .filter(|element| read_string(element, "AXRole").as_deref() == Some("AXButton"))
        .filter(|element| read_string(element, "AXTitle").as_deref() == Some("取消"))
        .collect::<Vec<_>>();
    if buttons.len() != 1 {
        bail!("expected exactly one dialog cancellation button");
    }
    buttons[0]
        .perform_action("AXPress")
        .map_err(|error| anyhow::anyhow!("failed to cancel broker dialog: {error:?}"))
}

fn prompt_observation(observation: &OcrObservation) -> bool {
    ["确认", "确定", "取消", "委托", "成功", "失败", "提示"]
        .iter()
        .any(|keyword| observation.text.contains(keyword))
}
