//! Captcha image handling: crop, upscale, recognize, normalize.
//!
//! Recognition uses the inbox `Windows.Media.Ocr` engine (on-device, no
//! network, no key). Only 4-digit codes are ever accepted; anything else is a
//! loud failure so a misread can never be submitted as a guess.

use anyhow::{Context, Result, bail};

use super::super::popups::Frame;

/// Pixel size the code crop is upscaled to before recognition; the source box
/// is small (~120x40) and the engine is far more reliable at 2x.
const UPSCALE: usize = 2;

/// Crops `frame` to `(x, y, w, h)`, clamped to the frame bounds.
pub fn crop(frame: &Frame, x: i32, y: i32, w: i32, h: i32) -> Option<Frame> {
    let (fw, fh) = (frame.width as i32, frame.height as i32);
    let x0 = x.clamp(0, fw).max(0) as usize;
    let y0 = y.clamp(0, fh).max(0) as usize;
    let x1 = (x + w).clamp(0, fw).max(0) as usize;
    let y1 = (y + h).clamp(0, fh).max(0) as usize;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut bgra = Vec::with_capacity(cw * ch * 4);
    for row in y0..y1 {
        for col in x0..x1 {
            let (r, g, b) = frame.rgb(col, row);
            bgra.extend_from_slice(&[b, g, r, 255]);
        }
    }
    Frame::from_pixels(cw, ch, bgra)
}

/// Nearest-neighbor 2x upscale of `frame`.
pub fn upscale2(frame: &Frame) -> Option<Frame> {
    let (fw, fh) = (frame.width, frame.height);
    if fw == 0 || fh == 0 {
        return None;
    }
    let (w2, h2) = (fw * UPSCALE, fh * UPSCALE);
    let mut bgra = Vec::with_capacity(w2 * h2 * 4);
    for row in 0..h2 {
        for col in 0..w2 {
            let (r, g, b) = frame.rgb(col / UPSCALE, row / UPSCALE);
            bgra.extend_from_slice(&[b, g, r, 255]);
        }
    }
    Frame::from_pixels(w2, h2, bgra)
}

/// Writes `frame` as a top-down 32-bit BMP for offline inspection.
/// Only used behind `STOCK_OPERATOR_DEBUG_CAPTCHA=1`; never in normal runs.
pub fn save_bmp(path: &std::path::Path, frame: &Frame) -> Result<()> {
    use std::io::Write;
    let (w, h) = (frame.width, frame.height);
    if w == 0 || h == 0 {
        bail!("refusing to dump an empty frame");
    }
    let row_bytes = w * 4;
    let pixel_bytes = row_bytes * h;
    let file_size = 14u32 + 40 + pixel_bytes as u32;
    let mut out = Vec::with_capacity(file_size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for row in (0..h).rev() {
        for col in 0..w {
            let (r, g, b) = frame.rgb(col, row);
            out.extend_from_slice(&[b, g, r, 255]);
        }
    }
    std::fs::File::create(path)
        .with_context(|| format!("captcha debug dump unavailable: {}", path.display()))?
        .write_all(&out)
        .context("captcha debug dump write failed")?;
    Ok(())
}

/// Normalizes raw OCR text to a 4-digit code.
pub fn normalize_code(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()) {
        Some(digits)
    } else {
        None
    }
}

/// Recognizes the code in `frame` (already cropped to the code box).
pub fn recognize_digits(frame: &Frame) -> Result<String> {
    init_com_once();
    let big =
        upscale2(frame).context("captcha crop could not be upscaled for recognition")?;
    let (w, h) = (big.width, big.height);
    let mut bgra = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        for col in 0..w {
            let (r, g, b) = big.rgb(col, row);
            bgra.extend_from_slice(&[b, g, r, 255]);
        }
    }
    let bitmap = software_bitmap(w as u32, h as u32, &bgra)?;
    let engine = ocr_engine()?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .context("OCR recognition could not start")?
        .get()
        .context("OCR recognition did not complete")?;
    let raw = result.Text().context("OCR result has no text")?.to_string();
    normalize_code(&raw)
        .with_context(|| format!("OCR did not yield 4 digits (text length {})", raw.trim().len()))
}

fn init_com_once() {
    use std::sync::OnceLock;
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        // Best effort: the OCR call sites surface real failures; an already
        // initialized apartment (even STA) must not fail the read path here.
        let _ = unsafe {
            windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            )
        };
    });
}

fn software_bitmap(
    width: u32,
    height: u32,
    bgra: &[u8],
) -> Result<windows::Graphics::Imaging::SoftwareBitmap> {
    use windows::{
        Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
        Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream},
    };
    let total = bgra.len();
    if total == 0 || total != width as usize * height as usize * 4 {
        bail!("captcha pixel buffer has an impossible size");
    }
    let stream = InMemoryRandomAccessStream::new().context("OCR stream unavailable")?;
    {
        let writer =
            DataWriter::CreateDataWriter(&stream).context("OCR writer unavailable")?;
        writer.WriteBytes(bgra).context("OCR write failed")?;
        writer.StoreAsync().context("OCR store failed")?.get().context("OCR store did not complete")?;
        writer.DetachStream().context("OCR detach failed")?;
    }
    let input = stream.GetInputStreamAt(0).context("OCR stream rewind failed")?;
    let reader = DataReader::CreateDataReader(&input).context("OCR reader unavailable")?;
    reader
        .LoadAsync(total as u32)
        .context("OCR load failed")?
        .get()
        .context("OCR load did not complete")?;
    let buffer = reader.DetachBuffer().context("OCR buffer detach failed")?;
    SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, width as i32, height as i32)
        .context("OCR bitmap creation failed")
}

fn ocr_engine() -> Result<windows::Media::Ocr::OcrEngine> {
    use windows::{Globalization::Language, Media::Ocr::OcrEngine};
    if let Ok(language) = Language::CreateLanguage(&"en".into()) {
        if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&language) {
            return Ok(engine);
        }
    }
    OcrEngine::TryCreateFromUserProfileLanguages()
        .context("no usable Windows OCR engine (need any Latin-capable OCR language)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscale_doubles_both_dimensions() {
        let frame = Frame::from_pixels(3, 2, vec![200u8; 3 * 2 * 4]).expect("frame");
        let big = upscale2(&frame).expect("upscaled");
        assert_eq!((big.width, big.height), (6, 4));
    }
}
