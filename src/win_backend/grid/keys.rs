//! Focus and synthesized keys for the grid read.
//!
//! Only `Ctrl+A` and `Ctrl+C` (to copy the focused grid) are ever synthesized,
//! and every event is refused unless the terminal owns the foreground window,
//! so a keystroke can never be delivered to another application. `Tab`,
//! panel shortcuts, `Enter` and any control activation are absent by
//! construction: grid focus is established once by the operator clicking the
//! table, and the copied payload itself decides whether the grid was reached.
//!
//! The terminal's 和讯 panes track keyboard focus inside the pane window, so
//! `GUITHREADINFO` reports no focused child while the terminal is active: the
//! caller decides from the copied payload instead of from the focus class.

use std::{mem::size_of, thread::sleep, time::Duration};

use anyhow::{Result, bail};

/// Class of the owner-drawn grid control that holds the table.
pub const GRID_CLASS_PREFIX: &str = "CVirtualGridCtrl";
/// Pause after a key event before the next state is inspected.
const KEY_SETTLE_MS: u64 = 150;

const KEYEVENTF_KEYUP: u32 = 0x0002;
const INPUT_KEYBOARD: u32 = 1;
const VK_CONTROL: u16 = 0x11;
pub(crate) const VK_A: u16 = 0x41;
pub(crate) const VK_C: u16 = 0x43;

/// Confirms the terminal can receive keys: restored and in front.
///
/// A minimized window stays "foreground" for Win32 while receiving no keyboard
/// input at all, so it is refused explicitly instead of sending keys into the
/// void. Background processes cannot always take the foreground; the caller then
/// gets an error instead of keys sent to whatever window is active.
pub fn ensure_readable(main_hwnd: isize) -> Result<()> {
    if unsafe { ffi::IsIconic(main_hwnd) } != 0 {
        bail!("the 至胜 terminal is minimized: restore it before reading the grid");
    }
    if unsafe { ffi::GetForegroundWindow() } == main_hwnd {
        return Ok(());
    }
    unsafe { ffi::SetForegroundWindow(main_hwnd) };
    sleep(Duration::from_millis(KEY_SETTLE_MS));
    if unsafe { ffi::GetForegroundWindow() } != main_hwnd {
        bail!("the 至胜 terminal is not the foreground window: bring it to the front and retry");
    }
    Ok(())
}

/// Sends `Ctrl+<vk>` to the focused control.
pub fn press_ctrl(main_hwnd: isize, vk: u16) -> Result<()> {
    send(
        main_hwnd,
        &[
            key(VK_CONTROL, 0),
            key(vk, 0),
            key(vk, KEYEVENTF_KEYUP),
            key(VK_CONTROL, KEYEVENTF_KEYUP),
        ],
    )?;
    sleep(Duration::from_millis(KEY_SETTLE_MS));
    Ok(())
}

fn send(target: isize, inputs: &[Input]) -> Result<()> {
    if unsafe { ffi::GetForegroundWindow() } != target {
        bail!("refusing to send keys: the 至胜 terminal is not the foreground window");
    }
    let sent =
        unsafe { ffi::SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<Input>() as i32) };
    if sent as usize != inputs.len() {
        bail!("SendInput delivered {sent}/{} events", inputs.len());
    }
    Ok(())
}

fn key(vk: u16, flags: u32) -> Input {
    Input {
        kind: INPUT_KEYBOARD,
        padding: 0,
        key: KeyInput {
            vk,
            scan: 0,
            flags,
            time: 0,
            extra: 0,
            padding: 0,
        },
    }
}

/// `INPUT` is 40 bytes on x64: a `u32` type tag, 4 bytes of padding, and the
/// 32-byte union member. Only the keyboard member is ever sent here.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Input {
    kind: u32,
    padding: u32,
    key: KeyInput,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct KeyInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: u64,
    padding: u64,
}

/// `SendInput` is declared here with this module's own `INPUT` layout; the same
/// `user32` entry point is declared in `popups::input` with a layout of its own,
/// which rustc reports as a benign `clashing_extern_declarations` (P3 follow-up:
/// share one input helper instead of two declarations).
#[allow(non_snake_case)]
mod ffi {
    use super::Input;

    pub type Bool = i32;
    pub type Hwnd = isize;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetForegroundWindow() -> Hwnd;
        pub fn SetForegroundWindow(hwnd: Hwnd) -> Bool;
        pub fn IsIconic(hwnd: Hwnd) -> Bool;
        pub fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_layout_matches_the_win32_abi() {
        assert_eq!(std::mem::size_of::<Input>(), 40);
        assert_eq!(std::mem::size_of::<KeyInput>(), 32);
    }

    #[test]
    fn grid_class_is_named_after_the_owner_drawn_control() {
        assert_eq!(GRID_CLASS_PREFIX, "CVirtualGridCtrl");
    }
}
