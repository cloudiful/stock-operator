use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::reader::PageReader;

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct OcrObservation {
    pub text: String,
    pub confidence: f32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
pub struct OcrSnapshot {
    pub pid: i32,
    pub window_id: u32,
    pub window_x: f64,
    pub window_y: f64,
    pub window_width: f64,
    pub window_height: f64,
    pub width: i64,
    pub height: i64,
    pub observations: Vec<OcrObservation>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HelperOutput {
    pid: i32,
    #[serde(rename = "windowID")]
    window_id: u32,
    #[serde(rename = "windowX")]
    window_x: f64,
    #[serde(rename = "windowY")]
    window_y: f64,
    #[serde(rename = "windowWidth")]
    window_width: f64,
    #[serde(rename = "windowHeight")]
    window_height: f64,
    width: i64,
    height: i64,
    observations: Vec<OcrObservation>,
    warnings: Vec<String>,
}

impl PageReader {
    pub fn ocr_visible_text(&self) -> Result<OcrSnapshot> {
        let status = self.target_status()?;
        let Some(pid) = status.target_pid else {
            bail!("target process is not running");
        };
        let helper = ocr_helper_path()?;
        let output = Command::new(&helper)
            .args(["--pid", &pid.to_string()])
            .output()
            .context("failed to execute macOS Vision OCR helper")?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            bail!("macOS Vision OCR helper failed: {message}");
        }
        let mut result = serde_json::from_slice::<HelperOutput>(&output.stdout)
            .context("OCR helper returned invalid JSON")?;
        let mut warnings = result.warnings;
        let mut redacted_count = 0;
        for observation in &mut result.observations {
            let (text, redacted) = redact_sensitive_numeric_text(&observation.text);
            observation.text = text;
            redacted_count += usize::from(redacted);
        }
        if redacted_count > 0 {
            warnings.push(format!(
                "redacted {redacted_count} long numeric OCR observation(s)"
            ));
        }
        Ok(OcrSnapshot {
            pid: result.pid,
            window_id: result.window_id,
            window_x: result.window_x,
            window_y: result.window_y,
            window_width: result.window_width,
            window_height: result.window_height,
            width: result.width,
            height: result.height,
            observations: result.observations,
            warnings,
        })
    }
}

fn ocr_helper_path() -> Result<PathBuf> {
    if let Ok(executable) = std::env::current_exe()
        && let Some(contents) = executable.parent().and_then(|macos| macos.parent())
    {
        let bundled = contents.join("Helpers/window-ocr");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    option_env!("STOCK_OPERATOR_OCR_HELPER")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .context("OCR helper is not available in this build or app bundle")
}

fn redact_sensitive_numeric_text(text: &str) -> (String, bool) {
    let compact = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let digit_count = compact
        .chars()
        .filter(|character| character.is_ascii_digit())
        .count();
    let has_decimal = compact.contains('.') || compact.contains('．');
    if digit_count >= 8 && !has_decimal {
        ("[REDACTED_NUMERIC]".to_string(), true)
    } else {
        (text.to_string(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::redact_sensitive_numeric_text;

    #[test]
    fn redacts_mixed_account_text() {
        assert_eq!(
            redact_sensitive_numeric_text("沪A A842988848"),
            ("[REDACTED_NUMERIC]".to_string(), true)
        );
    }

    #[test]
    fn keeps_short_numeric_market_text() {
        assert_eq!(
            redact_sensitive_numeric_text("53.46"),
            ("53.46".to_string(), false)
        );
    }

    #[test]
    fn redacted_ocr_values_are_not_exact() {
        let value =
            super::super::types::ObservedText::from_ocr("[REDACTED_NUMERIC]".to_string(), 1.0);
        assert_eq!(value.quality, super::super::types::DataQuality::Partial);
    }
}
