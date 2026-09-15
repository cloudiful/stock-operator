//! Clipboard access for the grid copy.
//!
//! The grid is copied with `Ctrl+C`, so the payload is read back from the
//! clipboard: `CF_UNICODETEXT` first, then `CF_TEXT` decoded through the system
//! ANSI code page (GBK on a Chinese Windows). The `导出到excel` fallback from the
//! task plan stays unused on purpose — it needs a click, and this phase only
//! sends `Tab`/`Ctrl+A`/`Ctrl+C`, so an unavailable clipboard is an error.

use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};

/// Upper bound for the terminal to publish the copied table.
const CLIPBOARD_TIMEOUT_MS: u64 = 800;
/// Clipboard-open retry delay and payload poll interval.
const POLL_MS: u64 = 100;
/// Clipboard-open attempts before giving up.
const OPEN_ATTEMPTS: usize = 10;

const CF_TEXT: u32 = 1;
const CF_UNICODETEXT: u32 = 13;

/// Empties the clipboard so a stale copy cannot pass as fresh grid data.
pub fn clear() -> Result<()> {
    open()?;
    unsafe {
        ffi::EmptyClipboard();
        ffi::CloseClipboard();
    }
    Ok(())
}

/// Waits for the copied grid text and returns it.
///
/// An empty or unreadable clipboard keeps being retried until
/// [`CLIPBOARD_TIMEOUT_MS`], then fails loudly; a supposedly successful copy
/// never turns into an empty table.
pub fn wait_for_text() -> Result<String> {
    let deadline = Instant::now() + Duration::from_millis(CLIPBOARD_TIMEOUT_MS);
    loop {
        let last_error = match read() {
            Ok(text) if !text.trim().is_empty() => return Ok(text),
            Ok(_) => "the clipboard text was empty".to_string(),
            Err(error) => format!("{error:#}"),
        };
        if Instant::now() >= deadline {
            bail!("no grid text appeared on the clipboard within {CLIPBOARD_TIMEOUT_MS} ms ({last_error})");
        }
        sleep(Duration::from_millis(POLL_MS));
    }
}

fn read() -> Result<String> {
    open()?;
    let payload = payload();
    unsafe { ffi::CloseClipboard() };
    payload
}

fn open() -> Result<()> {
    for _ in 0..OPEN_ATTEMPTS {
        if unsafe { ffi::OpenClipboard(0) } != 0 {
            return Ok(());
        }
        sleep(Duration::from_millis(POLL_MS));
    }
    bail!("the clipboard was held by another process for {OPEN_ATTEMPTS} attempts")
}

fn payload() -> Result<String> {
    if unsafe { ffi::IsClipboardFormatAvailable(CF_UNICODETEXT) } != 0 {
        let handle = unsafe { ffi::GetClipboardData(CF_UNICODETEXT) };
        if handle == 0 {
            bail!("the clipboard reported CF_UNICODETEXT but returned no handle");
        }
        return unsafe { utf16_data(handle) };
    }
    if unsafe { ffi::IsClipboardFormatAvailable(CF_TEXT) } != 0 {
        let handle = unsafe { ffi::GetClipboardData(CF_TEXT) };
        if handle == 0 {
            bail!("the clipboard reported CF_TEXT but returned no handle");
        }
        return unsafe { ansi_data(handle) };
    }
    bail!("the clipboard held neither CF_UNICODETEXT nor CF_TEXT")
}

unsafe fn utf16_data(handle: isize) -> Result<String> {
    let units = unsafe { ffi::GlobalSize(handle) } / 2;
    let pointer = unsafe { ffi::GlobalLock(handle) } as *const u16;
    if pointer.is_null() || units == 0 {
        bail!("the clipboard text memory was not readable");
    }
    let raw = unsafe { std::slice::from_raw_parts(pointer, units) };
    let text = decode_utf16(raw);
    unsafe { ffi::GlobalUnlock(handle) };
    Ok(text)
}

unsafe fn ansi_data(handle: isize) -> Result<String> {
    let bytes = unsafe { ffi::GlobalSize(handle) };
    let pointer = unsafe { ffi::GlobalLock(handle) } as *const u8;
    if pointer.is_null() || bytes == 0 {
        bail!("the clipboard text memory was not readable");
    }
    let raw = unsafe { std::slice::from_raw_parts(pointer, bytes) };
    let text = ansi_to_string(raw);
    unsafe { ffi::GlobalUnlock(handle) };
    text
}

/// Decodes a NUL-terminated UTF-16 buffer.
pub fn decode_utf16(raw: &[u16]) -> String {
    let end = raw.iter().position(|unit| *unit == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}

/// Decodes a NUL-terminated buffer in the system ANSI code page.
pub fn ansi_to_string(raw: &[u8]) -> Result<String> {
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    if end == 0 {
        return Ok(String::new());
    }
    let input = raw.as_ptr();
    let length = end as i32;
    let needed = unsafe { ffi::MultiByteToWideChar(0, 0, input, length, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        bail!("the CF_TEXT payload was not decodable in the system code page");
    }
    let mut buffer = vec![0u16; needed as usize];
    let written =
        unsafe { ffi::MultiByteToWideChar(0, 0, input, length, buffer.as_mut_ptr(), needed) };
    if written <= 0 {
        bail!("the CF_TEXT payload was not decodable in the system code page");
    }
    buffer.truncate(written as usize);
    Ok(String::from_utf16_lossy(&buffer))
}

#[allow(non_snake_case)]
mod ffi {
    pub type Bool = i32;
    pub type Hwnd = isize;
    pub type Handle = isize;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn OpenClipboard(owner: Hwnd) -> Bool;
        pub fn EmptyClipboard() -> Bool;
        pub fn CloseClipboard() -> Bool;
        pub fn IsClipboardFormatAvailable(format: u32) -> Bool;
        pub fn GetClipboardData(format: u32) -> Handle;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GlobalLock(handle: Handle) -> *mut u8;
        pub fn GlobalUnlock(handle: Handle) -> Bool;
        pub fn GlobalSize(handle: Handle) -> usize;
        pub fn MultiByteToWideChar(
            codepage: u32,
            flags: u32,
            input: *const u8,
            input_len: i32,
            output: *mut u16,
            output_len: i32,
        ) -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_payload_stops_at_the_terminator() {
        let raw = [0x8bc1u16, 0x5238, 0x0009, 0x0000, 0x8bc1];
        assert_eq!(decode_utf16(&raw), "证券\t");
        assert_eq!(decode_utf16(&[0x0000, 0x8bc1]), "");
        assert_eq!(decode_utf16(&[]), "");
    }

    /// ASCII is a subset of every system code page, so this stays valid whatever
    /// the machine's ANSI code page is.
    #[test]
    fn ansi_payload_decodes_in_the_system_code_page() {
        assert_eq!(
            ansi_to_string(b"600018\t10.55\t0\x00").expect("decodable"),
            "600018\t10.55\t0"
        );
        assert_eq!(ansi_to_string(b"\x00").expect("empty"), "");
        assert_eq!(ansi_to_string(&[]).expect("empty"), "");
    }
}
