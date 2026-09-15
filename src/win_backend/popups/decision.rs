//! Dismissal decision and verification for the announcement overlay.
//!
//! Kept free of OS calls so the HSV thresholds and the frame diff rule are
//! unit-testable against fixture frames.

use anyhow::{Context, Result, bail};

use super::image::{self, Blob, Frame};

/// HSV band (hue degrees, saturation, value) of the overlay's orange `确定`
/// button. Hardcoded on purpose: the acceptance criteria forbid making it
/// configurable.
pub const ORANGE_BTN_HSV_LO: (f64, f64, f64) = (15.0, 0.45, 0.55);
pub const ORANGE_BTN_HSV_HI: (f64, f64, f64) = (42.0, 1.0, 1.0);
/// `今日不再提示` checkbox, relative to the orange button's centre.
pub const NAG_OFFSET_DX: i32 = 150;
pub const NAG_OFFSET_DY: i32 = 46;
/// A settled frame may differ by at most this share of its pixels (`0.1 %`).
pub const DIFF_RATIO_LIMIT: f64 = 0.001;

/// Smallest solid orange blob accepted as the `确定` button.
const BTN_MIN_AREA: usize = 400;
const BTN_MIN_WIDTH: i32 = 24;
const BTN_MIN_HEIGHT: i32 = 12;
const BTN_ASPECT_LO: f64 = 1.2;
const BTN_ASPECT_HI: f64 = 8.0;
/// Blobs must be this solid to count as button pixels at all; page text renders
/// as thin anti-aliased slivers instead (largest live blob was 18 px, 2026-09-15).
const MIN_SOLID_FILL: f64 = 0.6;
/// A single solid blob this large is required before the `Enter` fallback may be
/// sent without matching button geometry.
const FALLBACK_MIN_BLOB_AREA: usize = 400;

/// What the capture says to do, decided before any input is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PopupPlan {
    /// Nothing overlay-like on screen: send no input at all.
    None,
    Click {
        /// `今日不再提示` in client coordinates, absent when it is off-screen.
        nag: Option<(i32, i32)>,
        /// Orange `确定` centre in client coordinates.
        button: (i32, i32),
    },
    /// Confirmed orange pixels but no button geometry: one `Enter` fallback.
    Enter,
}

pub(super) fn plan_for(frame: &Frame) -> PopupPlan {
    let blobs = image::blobs(frame, ORANGE_BTN_HSV_LO, ORANGE_BTN_HSV_HI);
    if let Some(blob) = blobs
        .iter()
        .filter(|blob| is_button(**blob))
        .max_by_key(|blob| blob.area)
    {
        let (x, y) = blob.center();
        let nag = (x + NAG_OFFSET_DX, y + NAG_OFFSET_DY);
        return PopupPlan::Click {
            nag: within(frame, nag).then_some(nag),
            button: (x, y),
        };
    }
    let orange = blobs
        .iter()
        .filter(|blob| blob.fill_ratio() >= MIN_SOLID_FILL)
        .map(|blob| blob.area)
        .max()
        .unwrap_or(0);
    if orange >= FALLBACK_MIN_BLOB_AREA {
        PopupPlan::Enter
    } else {
        PopupPlan::None
    }
}

fn is_button(blob: Blob) -> bool {
    let aspect = blob.width() as f64 / blob.height().max(1) as f64;
    blob.area >= BTN_MIN_AREA
        && blob.width() >= BTN_MIN_WIDTH
        && blob.height() >= BTN_MIN_HEIGHT
        && (BTN_ASPECT_LO..=BTN_ASPECT_HI).contains(&aspect)
        && blob.fill_ratio() >= MIN_SOLID_FILL
}

fn within(frame: &Frame, point: (i32, i32)) -> bool {
    point.0 >= 0 && point.1 >= 0 && point.0 < frame.width as i32 && point.1 < frame.height as i32
}

/// Dismissal proof: the client area changed when the overlay went away, the
/// frame pair around the settling pause differs by less than
/// [`DIFF_RATIO_LIMIT`] (`0.1 %`), and the orange template no longer matches.
pub(super) fn verify_dismissed(
    before: &Frame,
    after_action: &Frame,
    settled: &Frame,
) -> Result<()> {
    let changed =
        image::diff_ratio(before, settled).context("dismissal frames have different sizes")?;
    if changed < DIFF_RATIO_LIMIT {
        bail!("client area did not change: the announcement may still be open (diff {changed:.6})");
    }
    let stability = image::diff_ratio(after_action, settled)
        .context("post-dismissal frames have different sizes")?;
    if stability >= DIFF_RATIO_LIMIT {
        bail!("client area is still changing after the dismissal clicks (diff {stability:.6})");
    }
    if let Some(blob) = image::blobs(settled, ORANGE_BTN_HSV_LO, ORANGE_BTN_HSV_HI)
        .iter()
        .filter(|blob| is_button(**blob))
        .max_by_key(|blob| blob.area)
    {
        bail!("orange 确定 template still matched at {:?}", blob.center());
    }
    Ok(())
}
