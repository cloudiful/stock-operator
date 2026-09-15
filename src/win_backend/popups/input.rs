//! Synthesized input for the dismissal actions.
//!
//! Only `SendInput`/`SetCursorPos` from `user32` are used, and every event is
//! refused unless the terminal owns the foreground window and the target point,
//! so an event can never be delivered to another application.

use anyhow::{Result, bail};

use super::screen::{is_foreground, point_belongs_to};

const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const VK_RETURN: u16 = 0x000D;
const INPUT_MOUSE: u32 = 0;
const INPUT_KEYBOARD: u32 = 1;

/// Input sink for the dismissal actions; [`LiveInput`] talks to the OS, tests
/// record the actions instead.
pub trait UiInput {
    fn click(&mut self, hwnd: isize, x: i32, y: i32) -> Result<()>;
    fn press_enter(&mut self, hwnd: isize) -> Result<()>;
}

/// Real input, refused unless the terminal is foreground and owns the point.
pub struct LiveInput;

impl UiInput for LiveInput {
    fn click(&mut self, hwnd: isize, x: i32, y: i32) -> Result<()> {
        if !is_foreground(hwnd) {
            bail!("refusing to click ({x},{y}): the terminal is not the foreground window");
        }
        if !point_belongs_to(hwnd, x, y) {
            bail!("refusing to click ({x},{y}): the point does not belong to the terminal");
        }
        if unsafe { ffi::SetCursorPos(x, y) } == 0 {
            bail!("SetCursorPos({x},{y}) failed");
        }
        send_inputs(&[
            mouse_input(MOUSEEVENTF_LEFTDOWN),
            mouse_input(MOUSEEVENTF_LEFTUP),
        ])
    }

    fn press_enter(&mut self, hwnd: isize) -> Result<()> {
        if !is_foreground(hwnd) {
            bail!("refusing to press Enter: the terminal is not the foreground window");
        }
        send_inputs(&[
            key_input(VK_RETURN, 0),
            key_input(VK_RETURN, KEYEVENTF_KEYUP),
        ])
    }
}

fn send_inputs(inputs: &[Input]) -> Result<()> {
    let sent = unsafe {
        ffi::SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<Input>() as i32,
        )
    };
    if sent as usize != inputs.len() {
        bail!("SendInput delivered {sent}/{} events", inputs.len());
    }
    Ok(())
}

fn mouse_input(flags: u32) -> Input {
    Input {
        kind: INPUT_MOUSE,
        payload: Payload {
            mouse: MouseInput {
                flags,
                ..MouseInput::default()
            },
        },
    }
}

fn key_input(vk: u16, flags: u32) -> Input {
    Input {
        kind: INPUT_KEYBOARD,
        payload: Payload {
            key: KeyInput {
                vk,
                flags,
                ..KeyInput::default()
            },
        },
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MouseInput {
    dx: i32,
    dy: i32,
    data: u32,
    flags: u32,
    time: u32,
    extra: u64,
}

/// Padded to the mouse payload size so both variants keep `INPUT` at 40 bytes.
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

#[repr(C)]
#[derive(Clone, Copy)]
union Payload {
    mouse: MouseInput,
    key: KeyInput,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Input {
    kind: u32,
    payload: Payload,
}

mod ffi {
    use super::Input;

    pub type Bool = i32;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn SetCursorPos(x: i32, y: i32) -> Bool;
        pub fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    }
}
