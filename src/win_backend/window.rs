//! Read-only Win32 window discovery for the 恒生/至胜 terminal (`xiadan.exe`).
//!
//! Only inspection APIs are used: `FindWindowW`, `EnumChildWindows`,
//! `GetClassNameW`, `GetWindowTextW`, `GetWindowRect`, `IsWindowVisible`,
//! `GetParent` and `GetWindowThreadProcessId`. No clicks, keystrokes, focus
//! changes or mutation messages are sent.

use anyhow::{Result, bail};

/// Main window class observed on the login-state terminal (2026-09-15).
pub const MAIN_WINDOW_CLASS: &str = "Afx:00230000:b:00010003:00000006:01100729";
/// Main window caption.
pub const MAIN_WINDOW_TITLE: &str = "中信证券至胜金融终端";

const CLASS_CAPACITY: usize = 256;
const TEXT_CAPACITY: usize = 4096;
const MAX_PARENT_WALK: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub fn vertical_overlap(self, other: Rect) -> i32 {
        (self.bottom.min(other.bottom) - self.top.max(other.top)).max(0)
    }

    /// True when both controls share a visual row (>= 50% of the shorter height).
    pub fn shares_row(self, other: Rect) -> bool {
        let min_height = self.height().min(other.height()).max(1);
        self.vertical_overlap(other) * 2 >= min_height
    }
}

/// One window discovered in the target process, with the fields needed for
/// label/value pairing and bounded traversal.
#[derive(Debug, Clone)]
pub struct RawWindow {
    pub handle: isize,
    pub parent: isize,
    pub depth: usize,
    pub class: String,
    pub text: String,
    pub visible: bool,
    pub rect: Rect,
}

/// Discovery order for [`find_main_window`]: exact recorded evidence first, then
/// the class-only and title-only fallbacks, with `None` meaning "no filter".
pub const MAIN_WINDOW_ATTEMPTS: [(Option<&str>, Option<&str>); 3] = [
    (Some(MAIN_WINDOW_CLASS), Some(MAIN_WINDOW_TITLE)),
    (Some(MAIN_WINDOW_CLASS), None),
    (None, Some(MAIN_WINDOW_TITLE)),
];

/// Finds the main terminal window: class + title, then class-only, then
/// title-only so a version bump that changes the class is still discovered.
///
/// The MFC class carries the module instance handle of the running process
/// (`Afx:00500000:b:...:00220257` on 2026-09-15 vs `Afx:00230000:b:...:01100729`
/// in the recorded evidence), so it changes on every launch and the title-only
/// attempt is the one that keeps discovery working. `FindWindowW` needs `NULL`
/// for "no filter": an empty string matches no window at all, which is why that
/// fallback must pass a null pointer rather than `[0u16]`.
pub fn find_main_window() -> Result<isize> {
    for (class, title) in MAIN_WINDOW_ATTEMPTS {
        let class = class.map(wide);
        let title = title.map(wide);
        let handle = unsafe {
            ffi::FindWindowW(
                class.as_ref().map_or(std::ptr::null(), |value| value.as_ptr()),
                title.as_ref().map_or(std::ptr::null(), |value| value.as_ptr()),
            )
        };
        if handle != 0 {
            return Ok(handle);
        }
    }
    bail!("main window not found (class={MAIN_WINDOW_CLASS}, title={MAIN_WINDOW_TITLE})")
}

pub fn main_window_pid(handle: isize) -> Option<u32> {
    let mut pid = 0u32;
    unsafe { ffi::GetWindowThreadProcessId(handle, &mut pid) };
    (pid != 0).then_some(pid)
}

/// Enumerates descendants of `parent`, stopping after `limit` handles.
pub fn bounded_children(parent: isize, limit: usize) -> Vec<RawWindow> {
    if parent == 0 || limit == 0 {
        return Vec::new();
    }
    let mut sink = Sink {
        handles: Vec::new(),
        limit,
    };
    unsafe {
        ffi::EnumChildWindows(parent, Some(collect), &mut sink as *mut Sink as isize);
    }
    sink.handles.into_iter().map(describe).collect()
}

pub fn describe(handle: isize) -> RawWindow {
    RawWindow {
        handle,
        parent: unsafe { ffi::GetParent(handle) },
        depth: window_depth(handle),
        class: window_class(handle),
        text: window_text(handle),
        visible: unsafe { ffi::IsWindowVisible(handle) != 0 },
        rect: window_rect(handle).unwrap_or(Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }),
    }
}

/// Nesting level from the desktop root, used to honour `max_depth`.
pub fn window_depth(handle: isize) -> usize {
    let mut depth = 0;
    let mut current = handle;
    while depth < MAX_PARENT_WALK {
        let parent = unsafe { ffi::GetParent(current) };
        if parent == 0 || parent == current {
            break;
        }
        current = parent;
        depth += 1;
    }
    depth
}

pub fn window_class(handle: isize) -> String {
    let mut buffer = [0u16; CLASS_CAPACITY];
    let len = unsafe { ffi::GetClassNameW(handle, buffer.as_mut_ptr(), buffer.len() as i32) };
    decode(&buffer, len)
}

pub fn window_text(handle: isize) -> String {
    let mut buffer = vec![0u16; TEXT_CAPACITY];
    let len = unsafe { ffi::GetWindowTextW(handle, buffer.as_mut_ptr(), buffer.len() as i32) };
    decode(&buffer, len)
}

pub fn window_rect(handle: isize) -> Option<Rect> {
    let mut raw = ffi::RawRect::default();
    let ok = unsafe { ffi::GetWindowRect(handle, &mut raw) };
    (ok != 0).then_some(Rect {
        left: raw.left,
        top: raw.top,
        right: raw.right,
        bottom: raw.bottom,
    })
}

fn decode(buffer: &[u16], len: i32) -> String {
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..(len as usize).min(buffer.len())])
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Sink {
    handles: Vec<isize>,
    limit: usize,
}

unsafe extern "system" fn collect(handle: isize, lparam: isize) -> ffi::Bool {
    // SAFETY: `lparam` is the `Sink` pointer passed by `bounded_children` on this thread.
    let sink = unsafe { &mut *(lparam as *mut Sink) };
    sink.handles.push(handle);
    (sink.handles.len() < sink.limit) as ffi::Bool
}

#[allow(non_snake_case)]
mod ffi {
    pub type Bool = i32;
    pub type Hwnd = isize;
    pub type Lparam = isize;
    pub type EnumProc = Option<unsafe extern "system" fn(Hwnd, Lparam) -> Bool>;

    #[repr(C)]
    #[derive(Default)]
    pub struct RawRect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn FindWindowW(class: *const u16, title: *const u16) -> Hwnd;
        pub fn EnumChildWindows(parent: Hwnd, callback: EnumProc, lparam: Lparam) -> Bool;
        pub fn GetClassNameW(handle: Hwnd, buffer: *mut u16, max: i32) -> i32;
        pub fn GetWindowTextW(handle: Hwnd, buffer: *mut u16, max: i32) -> i32;
        pub fn IsWindowVisible(handle: Hwnd) -> Bool;
        pub fn GetWindowThreadProcessId(handle: Hwnd, pid: *mut u32) -> u32;
        pub fn GetParent(handle: Hwnd) -> Hwnd;
        pub fn GetWindowRect(handle: Hwnd, rect: *mut RawRect) -> Bool;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression (2026-09-15): the fallbacks passed empty strings, which
    /// `FindWindowW` reads as a filter that matches no window, so a launch whose
    /// MFC class differed from the recorded evidence could never be found. The
    /// live terminal is only reachable through the title-only fallback, and it
    /// must ask for "no class filter", i.e. `None`.
    #[test]
    fn fallbacks_ask_for_no_filter_instead_of_an_empty_string() {
        assert_eq!(
            MAIN_WINDOW_ATTEMPTS[0],
            (Some(MAIN_WINDOW_CLASS), Some(MAIN_WINDOW_TITLE))
        );
        assert_eq!(MAIN_WINDOW_ATTEMPTS[1], (Some(MAIN_WINDOW_CLASS), None));
        assert_eq!(MAIN_WINDOW_ATTEMPTS[2], (None, Some(MAIN_WINDOW_TITLE)));
        assert!(
            MAIN_WINDOW_ATTEMPTS
                .iter()
                .all(|(class, title)| class.is_some() || title.is_some()),
            "an attempt must never filter by nothing at all"
        );
    }
}
