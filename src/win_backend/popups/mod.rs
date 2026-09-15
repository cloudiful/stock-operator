//! `营业部公告` overlay handling for the 恒生/至胜 terminal.
//!
//! The overlay is an in-window `htmlayout` layer without its own HWND, so it is
//! located by pixel template matching: client-area capture, then a hand-rolled
//! HSV match on the orange `确定` button (`decision` holds the hardcoded
//! thresholds). The only writes this module ever performs are the overlay's
//! `今日不再提示` checkbox, its orange `确定` button, or a single `Enter`
//! fallback — always followed by a frame-diff verification. The `版本过低`
//! dialog is reported but never closed.
//!
//! Module split: `image` holds frame storage and pixel math, `screen` holds the
//! `user32`/`gdi32` capture FFI, `input` holds the synthesized input, and
//! `decision` holds the plan and its verification.

mod decision;
mod image;
mod input;
mod screen;

#[cfg(test)]
mod tests;

use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::{read, window};

use self::decision::{PopupPlan, plan_for, verify_dismissed};
use self::input::UiInput;

const MAX_SCAN_WINDOWS: usize = 4096;
/// Pause after the dismissal input, before the first verification frame.
const DISMISS_WAIT_MS: u64 = 400;
/// Pause between the two verification frames.
const SETTLE_WAIT_MS: u64 = 400;
/// Pause between the `今日不再提示` click and the `确定` click.
const CLICK_SETTLE_MS: u64 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupOutcome {
    /// No announcement overlay on screen: no input was sent.
    None,
    /// The announcement overlay was closed and verified gone.
    DismissedAnnouncement,
    /// The `版本过低` dialog is open; live mutations must stay disabled.
    VersionLowBlocksMutations,
}

/// Closes a blocking announcement overlay when one is on screen.
///
/// Returns without writing anything when no announcement evidence is present,
/// and returns [`PopupOutcome::VersionLowBlocksMutations`] without touching the
/// `版本过低` dialog. Any dismissal that cannot be verified is an error, so
/// callers keep failing closed.
pub fn ensure_no_blocking_popup(main_hwnd: isize) -> Result<PopupOutcome> {
    let nodes = window::bounded_children(main_hwnd, MAX_SCAN_WINDOWS);
    if read::detect_version_low(&nodes) {
        return Ok(PopupOutcome::VersionLowBlocksMutations);
    }
    // Only the in-window overlay may be clicked. While a real dialog window is on
    // screen (e.g. the order confirmation) client-area pixels cannot be
    // attributed to the overlay, so the dismissal is refused and callers keep
    // failing closed instead of clicking blind.
    if let Some(pid) = window::main_window_pid(main_hwnd) {
        if let Some(dialog) = screen::visible_dialogs(pid).first() {
            bail!("terminal dialog {dialog:#x} is open: refusing to dismiss client-area pixels");
        }
    }
    let before = screen::capture_client(main_hwnd)?;
    let plan = plan_for(&before);
    if plan == PopupPlan::None {
        return Ok(PopupOutcome::None);
    }
    let origin = screen::client_origin(main_hwnd)?;
    execute_plan(&mut input::LiveInput, main_hwnd, origin, plan)?;
    sleep(Duration::from_millis(DISMISS_WAIT_MS));
    let after_action = screen::capture_client(main_hwnd)?;
    sleep(Duration::from_millis(SETTLE_WAIT_MS));
    let settled = screen::capture_client(main_hwnd)?;
    verify_dismissed(&before, &after_action, &settled)
        .with_context(|| format!("announcement overlay dismissal at origin {origin:?} failed"))?;
    Ok(PopupOutcome::DismissedAnnouncement)
}

/// Runs the plan in the fixed order: `今日不再提示` first, then `确定`, with a
/// single `Enter` as the only fallback. Client coordinates are translated to
/// screen coordinates through the client area's `origin`.
fn execute_plan(
    input: &mut dyn UiInput,
    hwnd: isize,
    origin: (i32, i32),
    plan: PopupPlan,
) -> Result<()> {
    match plan {
        PopupPlan::None => Ok(()),
        PopupPlan::Click { nag, button } => {
            if let Some((x, y)) = nag {
                input
                    .click(hwnd, origin.0 + x, origin.1 + y)
                    .context("clicking 今日不再提示 failed")?;
                sleep(Duration::from_millis(CLICK_SETTLE_MS));
            }
            input
                .click(hwnd, origin.0 + button.0, origin.1 + button.1)
                .context("clicking 确定 failed")
        }
        PopupPlan::Enter => input
            .press_enter(hwnd)
            .context("sending the fallback Enter failed"),
    }
}
