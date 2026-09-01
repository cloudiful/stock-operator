use std::{ffi::c_void, thread, time::Duration};

use anyhow::{Result, bail};

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateMouseEvent(
        source: *mut c_void,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> *mut c_void;
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}

pub(crate) fn click(x: f64, y: f64) -> Result<()> {
    click_sequence(x, y, 1)
}

pub(crate) fn double_click(x: f64, y: f64) -> Result<()> {
    click_sequence(x, y, 2)
}

fn click_sequence(x: f64, y: f64, count: i64) -> Result<()> {
    if !x.is_finite() || !y.is_finite() {
        bail!("mouse coordinates are not finite");
    }
    for click_state in 1..=count {
        post_click(x, y, click_state)?;
        if click_state < count {
            thread::sleep(Duration::from_millis(80));
        }
    }
    Ok(())
}

fn post_click(x: f64, y: f64, click_state: i64) -> Result<()> {
    let point = CGPoint { x, y };
    // CGEventType leftMouseDown/leftMouseUp are 1/2; left button and HID tap are 0.
    // CGEventField mouseEventClickState is 1.
    let down = unsafe { CGEventCreateMouseEvent(std::ptr::null_mut(), 1, point, 0) };
    if down.is_null() {
        bail!("could not create mouse-down event");
    }
    let up = unsafe { CGEventCreateMouseEvent(std::ptr::null_mut(), 2, point, 0) };
    if up.is_null() {
        unsafe { CFRelease(down) };
        bail!("could not create mouse-up event");
    }
    unsafe {
        CGEventSetIntegerValueField(down, 1, click_state);
        CGEventSetIntegerValueField(up, 1, click_state);
        CGEventPost(0, down);
        thread::sleep(Duration::from_millis(20));
        CGEventPost(0, up);
        CFRelease(down);
        CFRelease(up);
    }
    Ok(())
}
