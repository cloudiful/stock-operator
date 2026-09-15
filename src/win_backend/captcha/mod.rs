//! Copy-guard captcha solver for the terminal clipboard gate.
//!
//! Every clipboard copy out of the grid raises a modal dialog that only
//! releases the data after a 4-digit image code is typed in and confirmed.
//! This module detects exactly that dialog (sentinel text gate), recognizes
//! the code with the inbox OCR engine, fills it with `WM_SETTEXT`, presses
//! `确定` with `BM_CLICK` (the cursor never moves), and verifies the dialog is
//! gone. Budgets are hard: at most [`MAX_CODE_ATTEMPTS`] submitted codes per
//! call; a dialog that never shows an error and never closes is a loud failure
//! instead of another guess.

mod detect;
mod ocr;
#[cfg(test)]
mod tests;

pub use detect::{GuardDialog, find_copy_guard};

use std::{thread::sleep, time::{Duration, Instant}};

use anyhow::{Context, Result, bail};

use super::popups::{capture_client, client_origin};

/// Submitted codes per [`solve_copy_guard`] call.
pub const MAX_CODE_ATTEMPTS: u32 = 3;
/// How long a submitted code may take to dismiss the dialog.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

/// Outcome of [`solve_copy_guard`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolveOutcome {
    /// No copy-guard dialog was present; nothing was touched.
    Absent,
    /// A copy-guard dialog was solved and is gone.
    Solved,
}

impl SolveOutcome {
    pub fn solved(self) -> bool {
        matches!(self, Self::Solved)
    }
}

/// Solves the copy-guard dialog of `main_hwnd` when one is showing.
///
/// Returns [`SolveOutcome::Absent`] without touching anything when no guard
/// dialog is present. Fails loudly when a code cannot be recognized, the fill
/// cannot be verified, or the dialog survives [`MAX_CODE_ATTEMPTS`] codes.
pub fn solve_copy_guard(main_hwnd: isize) -> Result<SolveOutcome> {
    let mut solved_any = false;
    for _ in 0..MAX_CODE_ATTEMPTS {
        let Some(guard) = find_copy_guard(main_hwnd)? else {
            return Ok(if solved_any {
                SolveOutcome::Solved
            } else {
                SolveOutcome::Absent
            });
        };
        submit_one_code(&guard)?;
        solved_any = true;
        if wait_gone(guard.dialog) {
            return Ok(SolveOutcome::Solved);
        }
        // Still showing: only a visible error justifies a fresh code.
        if !shows_error_now(main_hwnd)? {
            bail!("copy-guard dialog survived a submitted code without an error hint");
        }
    }
    if find_copy_guard(main_hwnd)?.is_none() {
        return Ok(SolveOutcome::Solved);
    }
    bail!("copy-guard dialog still blocks copies after {MAX_CODE_ATTEMPTS} codes");
}

fn shows_error_now(main_hwnd: isize) -> Result<bool> {
    Ok(find_copy_guard(main_hwnd)?.is_some_and(|guard| guard.shows_error))
}

fn submit_one_code(guard: &GuardDialog) -> Result<()> {
    let frame = capture_client(guard.dialog)?;
    let (ox, oy) = client_origin(guard.dialog)?;
    let shot = cropped(&frame, guard, ox, oy)?;
    debug_dump(&frame, &shot, guard, ox, oy);
    // The one-time code itself is never logged; failures carry lengths only.
    let code = ocr::recognize_digits(&shot)
        .map_err(|error| anyhow::anyhow!("captcha code not recognized ({error:#})"))?;
    detect::set_edit_text(guard.edit, &code)?;
    detect::click_ok(guard.ok)?;
    Ok(())
}

/// Dumps the dialog capture and the code crop to `%TEMP%` when
/// `STOCK_OPERATOR_DEBUG_CAPTCHA=1` (failure diagnosis only).
fn debug_dump(
    frame: &super::popups::Frame,
    shot: &super::popups::Frame,
    guard: &GuardDialog,
    ox: i32,
    oy: i32,
) {
    if std::env::var("STOCK_OPERATOR_DEBUG_CAPTCHA").as_deref() != Ok("1") {
        return;
    }
    let dir = std::env::temp_dir();
    let _ = ocr::save_bmp(&dir.join("captcha-dialog.bmp"), frame);
    let _ = ocr::save_bmp(&dir.join("captcha-crop.bmp"), shot);
    // Numbers only: geometry trace for reconciling capture vs window rects.
    let win = crate::win_backend::window::window_rect(guard.dialog);
    tracing::info!(
        frame_w = frame.width,
        frame_h = frame.height,
        shot_w = shot.width,
        shot_h = shot.height,
        origin = format!("{ox},{oy}"),
        code = format!(
            "{},{},{},{}",
            guard.code_screen.left,
            guard.code_screen.top,
            guard.code_screen.right,
            guard.code_screen.bottom
        ),
        win = format!("{win:?}"),
        "captcha geometry"
    );
}

fn cropped(
    frame: &super::popups::Frame,
    guard: &GuardDialog,
    ox: i32,
    oy: i32,
) -> Result<super::popups::Frame> {
    let rect = guard.code_screen;
    ocr::crop(
        frame,
        rect.left - ox,
        rect.top - oy,
        rect.right - rect.left,
        rect.bottom - rect.top,
    )
    .with_context(|| "captcha code box falls outside the dialog capture")
}

fn wait_gone(dialog: isize) -> bool {
    let deadline = Instant::now() + CLOSE_TIMEOUT;
    while Instant::now() < deadline {
        if detect::dialog_gone(dialog) {
            return true;
        }
        sleep(Duration::from_millis(100));
    }
    detect::dialog_gone(dialog)
}
