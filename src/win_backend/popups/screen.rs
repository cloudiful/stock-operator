//! Client-area capture for the terminal window.
//!
//! All access goes through bare `user32`/`gdi32` FFI: no image or screenshot
//! crate is involved. Capture is render-based (`PrintWindow`), and the screen
//! `BitBlt` fallback only runs while the terminal is the foreground window.

use std::ffi::c_void;
use std::ptr;

use anyhow::{Result, anyhow, bail};

use crate::win_backend::window;

use super::image::Frame;

const PW_CLIENT_ONLY: u32 = 0x0000_0001;
const PW_RENDER_FULL_CONTENT: u32 = 0x0000_0002;
const SRCCOPY: u32 = 0x00CC_0020;
const DIB_RGB_COLORS: u32 = 0;
const GA_ROOT: u32 = 2;
const DIALOG_CLASS: &str = "#32770";
/// Refuse implausible client sizes before allocating a DIB.
const MAX_CLIENT_SIDE: i32 = 20_000;

/// Client-area size in device pixels.
pub fn client_size(hwnd: isize) -> Result<(i32, i32)> {
    let mut rect = ffi::RawRect::default();
    if unsafe { ffi::GetClientRect(hwnd, &mut rect) } == 0 {
        bail!("GetClientRect failed for window {hwnd:#x}");
    }
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 {
        bail!("terminal client area is empty ({width}x{height})");
    }
    if width > MAX_CLIENT_SIDE || height > MAX_CLIENT_SIDE {
        bail!("terminal client area is implausibly large ({width}x{height})");
    }
    Ok((width, height))
}

/// Client-area top-left corner in screen coordinates.
pub fn client_origin(hwnd: isize) -> Result<(i32, i32)> {
    let mut point = ffi::Point::default();
    if unsafe { ffi::ClientToScreen(hwnd, &mut point) } == 0 {
        bail!("ClientToScreen failed for window {hwnd:#x}");
    }
    Ok((point.x, point.y))
}

/// One frame of the client area.
///
/// `PrintWindow` renders the window itself, so the frame is valid while the
/// window is occluded; a uniform (blank) render is retried from the screen only
/// when the terminal is foreground, which keeps pixels from other apps out.
pub fn capture_client(hwnd: isize) -> Result<Frame> {
    let (width, height) = client_size(hwnd)?;
    let rendered = with_dib(width, height, |dc| unsafe {
        ffi::PrintWindow(hwnd, dc, PW_CLIENT_ONLY | PW_RENDER_FULL_CONTENT)
    })?;
    if !rendered.is_uniform() || !is_foreground(hwnd) {
        return Ok(rendered);
    }
    let (x, y) = client_origin(hwnd)?;
    Ok(with_dib(width, height, |dc| unsafe {
        let screen = ffi::GetDC(0);
        if screen == 0 {
            return 0;
        }
        let ok = ffi::BitBlt(dc, 0, 0, width, height, screen, x, y, SRCCOPY);
        ffi::ReleaseDC(0, screen);
        ok
    })
    .unwrap_or(rendered))
}

/// True while `hwnd` owns the foreground window, i.e. synthetic input lands on it.
pub fn is_foreground(hwnd: isize) -> bool {
    let foreground = unsafe { ffi::GetForegroundWindow() };
    foreground != 0 && unsafe { ffi::GetAncestor(foreground, GA_ROOT) } == hwnd
}

/// True when the screen point is covered by `hwnd` or one of its descendants.
pub fn point_belongs_to(hwnd: isize, x: i32, y: i32) -> bool {
    let window = unsafe { ffi::WindowFromPoint(ffi::Point { x, y }) };
    window != 0 && unsafe { ffi::GetAncestor(window, GA_ROOT) } == hwnd
}

/// Visible dialog windows (`#32770`) owned by process `pid`.
///
/// A dialog window covers the client area without being part of it, so while one
/// is on screen the captured pixels cannot be attributed to the in-window
/// announcement overlay and no click may be derived from them.
pub fn visible_dialogs(pid: u32) -> Vec<isize> {
    let mut sink = DialogSink {
        pid,
        found: Vec::new(),
    };
    unsafe { ffi::EnumWindows(Some(collect_dialog), &mut sink as *mut DialogSink as isize) };
    sink.found
}

struct DialogSink {
    pid: u32,
    found: Vec<isize>,
}

unsafe extern "system" fn collect_dialog(window: isize, lparam: isize) -> ffi::Bool {
    // SAFETY: `lparam` is the `DialogSink` pointer passed by `visible_dialogs` on this thread.
    let sink = unsafe { &mut *(lparam as *mut DialogSink) };
    let mut owner = 0u32;
    unsafe { ffi::GetWindowThreadProcessId(window, &mut owner) };
    if owner == sink.pid
        && unsafe { ffi::IsWindowVisible(window) } != 0
        && window::window_class(window) == DIALOG_CLASS
    {
        sink.found.push(window);
    }
    1
}

/// Runs `draw` against a top-down 32-bit DIB of `width x height` and returns the pixels.
fn with_dib(width: i32, height: i32, draw: impl FnOnce(isize) -> i32) -> Result<Frame> {
    let screen = unsafe { ffi::GetDC(0) };
    if screen == 0 {
        bail!("screen DC unavailable");
    }
    let memory = unsafe { ffi::CreateCompatibleDC(screen) };
    let info = BitmapInfo::new(width, height);
    let mut bits: *mut c_void = ptr::null_mut();
    let bitmap = unsafe { ffi::CreateDIBSection(memory, &info, DIB_RGB_COLORS, &mut bits, 0, 0) };
    if memory == 0 || bitmap == 0 {
        if bitmap != 0 {
            unsafe { ffi::DeleteObject(bitmap) };
        }
        if memory != 0 {
            unsafe { ffi::DeleteDC(memory) };
        }
        unsafe { ffi::ReleaseDC(0, screen) };
        bail!("could not allocate a {width}x{height} DIB section");
    }
    let previous = unsafe { ffi::SelectObject(memory, bitmap) };
    let drawn = draw(memory);
    let mut bgra = vec![0u8; width as usize * height as usize * 4];
    if drawn != 0 && !bits.is_null() {
        // SAFETY: the DIB owns `width * height * 4` bytes for as long as `bitmap` lives.
        unsafe { ptr::copy_nonoverlapping(bits as *const u8, bgra.as_mut_ptr(), bgra.len()) };
    }
    unsafe {
        ffi::SelectObject(memory, previous);
        ffi::DeleteObject(bitmap);
        ffi::DeleteDC(memory);
        ffi::ReleaseDC(0, screen);
    }
    if drawn == 0 {
        bail!("drawing the terminal client area failed");
    }
    Frame::from_pixels(width as usize, height as usize, bgra)
        .ok_or_else(|| anyhow!("captured frame has an unexpected byte length"))
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    image_size: u32,
    x_pels: i32,
    y_pels: i32,
    colors_used: u32,
    colors_important: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RgbQuad {
    blue: u8,
    green: u8,
    red: u8,
    reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [RgbQuad; 3],
}

impl BitmapInfo {
    fn new(width: i32, height: i32) -> Self {
        Self {
            header: BitmapInfoHeader {
                size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                width,
                // Negative height: row 0 is the top row, matching the frame layout.
                height: -height,
                planes: 1,
                bit_count: 32,
                ..BitmapInfoHeader::default()
            },
            colors: [RgbQuad::default(); 3],
        }
    }
}

#[allow(non_snake_case)]
mod ffi {
    use std::ffi::c_void;

    use super::BitmapInfo;

    pub type Bool = i32;
    pub type Hwnd = isize;
    pub type Hdc = isize;
    pub type Hobject = isize;
    pub type Hbitmap = isize;
    pub type Hsection = isize;
    pub type Lparam = isize;
    pub type EnumProc = Option<unsafe extern "system" fn(Hwnd, Lparam) -> Bool>;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct RawRect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetClientRect(handle: Hwnd, rect: *mut RawRect) -> Bool;
        pub fn ClientToScreen(handle: Hwnd, point: *mut Point) -> Bool;
        pub fn PrintWindow(handle: Hwnd, dc: Hdc, flags: u32) -> Bool;
        pub fn GetForegroundWindow() -> Hwnd;
        pub fn WindowFromPoint(point: Point) -> Hwnd;
        pub fn GetAncestor(handle: Hwnd, flags: u32) -> Hwnd;
        pub fn EnumWindows(callback: EnumProc, lparam: Lparam) -> Bool;
        pub fn IsWindowVisible(handle: Hwnd) -> Bool;
        pub fn GetWindowThreadProcessId(handle: Hwnd, pid: *mut u32) -> u32;
        pub fn GetDC(handle: Hwnd) -> Hdc;
        pub fn ReleaseDC(handle: Hwnd, dc: Hdc) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        pub fn CreateCompatibleDC(dc: Hdc) -> Hdc;
        pub fn DeleteDC(dc: Hdc) -> Bool;
        pub fn CreateDIBSection(
            dc: Hdc,
            info: *const BitmapInfo,
            usage: u32,
            bits: *mut *mut c_void,
            section: Hsection,
            offset: u32,
        ) -> Hbitmap;
        pub fn SelectObject(dc: Hdc, object: Hobject) -> Hobject;
        pub fn DeleteObject(object: Hobject) -> Bool;
        #[allow(clippy::too_many_arguments)]
        pub fn BitBlt(
            dest: Hdc,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            src: Hdc,
            src_x: i32,
            src_y: i32,
            rop: u32,
        ) -> Bool;
    }
}
