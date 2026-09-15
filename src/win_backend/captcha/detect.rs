//! Discovery and message primitives for the terminal copy guard.
//!
//! After certain clipboard copies the terminal raises a modal `#32770` dialog
//! titled `提示` whose body contains [`COPY_GUARD_SENTINEL`]. Copying only
//! proceeds after a 4-digit image code is typed into its `Edit` and `确定` is
//! pressed. Nothing here touches any other dialog: discovery requires the
//! sentinel text, and the only messages ever sent are `WM_SETTEXT` to that
//! dialog's `Edit` plus `BM_CLICK` to its `确定` button.

use anyhow::{Context, Result, bail};

use super::super::{
    popups::visible_dialogs,
    window::{self, Rect},
};

/// Body text that uniquely identifies the copy-guard dialog.
pub const COPY_GUARD_SENTINEL: &str = "检测到您正在拷贝数据";
/// Label of the dialog's confirm button.
pub const CONFIRM_LABEL: &str = "确定";
/// Text that appears (hidden until used) when a submitted code was wrong.
pub const ERROR_TEXT: &str = "验证码错误";

/// Handles of a live copy-guard dialog plus the screen rect of the code image.
pub struct GuardDialog {
    pub dialog: isize,
    pub edit: isize,
    pub ok: isize,
    pub code_screen: Rect,
    pub shows_error: bool,
}

/// Finds the copy-guard dialog of `main_hwnd`, if one is currently visible.
///
/// Returns `Ok(None)` when no dialog carries the sentinel text. Never fails
/// just because the terminal shows some other dialog.
pub fn find_copy_guard(main_hwnd: isize) -> Result<Option<GuardDialog>> {
    let Some(pid) = window::main_window_pid(main_hwnd) else {
        bail!("terminal main window has no process id");
    };
    for dialog in visible_dialogs(pid) {
        if let Some(found) = inspect_dialog(dialog)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn inspect_dialog(dialog: isize) -> Result<Option<GuardDialog>> {
    let children = window::bounded_children(dialog, 128);
    let sentinel = children
        .iter()
        .any(|child| child.visible && child.class == "Static" && is_guard_text(&child.text));
    if !sentinel {
        return Ok(None);
    }
    let edit = children
        .iter()
        .find(|child| child.visible && child.class == "Edit")
        .context("copy-guard dialog has no visible Edit")?;
    let ok = children
        .iter()
        .find(|child| {
            child.visible && child.class == "Button" && child.text.trim() == CONFIRM_LABEL
        })
        .context("copy-guard dialog has no visible 确定 button")?;
    let shows_error = children.iter().any(|child| {
        child.visible && child.class == "Static" && child.text.contains(ERROR_TEXT)
    });
    let code_screen = code_rect(&children, &edit.rect);
    Ok(Some(GuardDialog {
        dialog,
        edit: edit.handle,
        ok: ok.handle,
        code_screen,
        shows_error,
    }))
}

/// Locates the white code-image box: an empty-text `Static` right of the `Edit`.
/// Falls back to a fixed crop right of the `Edit` so a restyled dialog still
/// yields an image instead of no image.
fn code_rect(children: &[window::RawWindow], edit: &Rect) -> Rect {
    let edit_h = (edit.bottom - edit.top).max(1);
    let mut best: Option<Rect> = None;
    let mut best_area = 0i32;
    for child in children.iter().filter(|child| {
        child.visible
            && child.class == "Static"
            && child.text.trim().is_empty()
            && child.rect.left >= edit.right - 30
            && child.rect.left <= edit.right + 200
    }) {
        let width = child.rect.right - child.rect.left;
        let height = (child.rect.bottom - child.rect.top).max(1);
        if height < edit_h / 2 || height > edit_h + edit_h / 2 || width < height + height / 2 {
            continue;
        }
        let area = width * height;
        if area > best_area {
            best_area = area;
            best = Some(child.rect);
        }
    }
    best.unwrap_or(Rect {
        left: edit.right + 8,
        top: edit.top - 8,
        right: edit.right + 178,
        bottom: edit.bottom + 8,
    })
}

/// Writes `code` into the dialog `Edit` and reads it back.
pub fn set_edit_text(edit: isize, code: &str) -> Result<()> {
    let wide: Vec<u16> = code.encode_utf16().chain(std::iter::once(0)).collect();
    let sent = unsafe { ffi::SendMessageW(edit, ffi::WM_SETTEXT, 0, wide.as_ptr() as isize) };
    if sent == 0 {
        bail!("WM_SETTEXT to the captcha Edit was refused");
    }
    let back = edit_text(edit)?;
    if back.trim() != code {
        bail!("captcha Edit readback mismatch after write");
    }
    Ok(())
}

fn edit_text(edit: isize) -> Result<String> {
    let len = unsafe { ffi::SendMessageW(edit, ffi::WM_GETTEXTLENGTH, 0, 0) } as usize;
    if len == 0 || len > 64 {
        bail!("captcha Edit has an implausible length ({len})");
    }
    let buf = vec![0u16; len + 1];
    unsafe { ffi::SendMessageW(edit, ffi::WM_GETTEXT, buf.len(), buf.as_ptr() as isize) };
    let end = buf.iter().position(|unit| *unit == 0).unwrap_or(len);
    String::from_utf16(&buf[..end]).context("captcha Edit text is not UTF-16")
}

/// Presses the dialog `确定` button without moving the cursor.
pub fn click_ok(ok: isize) -> Result<()> {
    unsafe { ffi::SendMessageW(ok, ffi::BM_CLICK, 0, 0) };
    Ok(())
}

/// Whether the dialog window is gone (closed counts as gone).
pub fn dialog_gone(dialog: isize) -> bool {
    unsafe { ffi::IsWindow(dialog) == 0 || ffi::IsWindowVisible(dialog) == 0 }
}

/// Whether `text` marks a copy-guard dialog body (pure helper, unit-tested).
pub fn is_guard_text(text: &str) -> bool {
    text.contains(COPY_GUARD_SENTINEL)
}

mod ffi {
    pub type Hwnd = isize;
    pub type Lparam = isize;
    pub type Bool = i32;
    pub const WM_SETTEXT: u32 = 0x000C;
    pub const WM_GETTEXT: u32 = 0x000D;
    pub const WM_GETTEXTLENGTH: u32 = 0x000E;
    pub const BM_CLICK: u32 = 0x00F5;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn SendMessageW(handle: Hwnd, msg: u32, wparam: usize, lparam: Lparam) -> Lparam;
        pub fn IsWindow(handle: Hwnd) -> Bool;
        pub fn IsWindowVisible(handle: Hwnd) -> Bool;
    }
}
