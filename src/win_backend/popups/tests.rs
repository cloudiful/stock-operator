//! Unit tests for the popups module: synthetic fixture frames instead of live
//! captures, so the hardcoded HSV thresholds, the dismissal order and the
//! frame-diff verification stay covered without a running terminal.

use anyhow::Result;

use super::decision::{
    NAG_OFFSET_DX, NAG_OFFSET_DY, ORANGE_BTN_HSV_HI, ORANGE_BTN_HSV_LO, PopupPlan, plan_for,
    verify_dismissed,
};
use super::execute_plan;
use super::image::{Frame, blobs, diff_ratio, rgb_to_hsv};
use super::input::UiInput;

const ORANGE: (u8, u8, u8) = (255, 140, 0);
const GREY: (u8, u8, u8) = (240, 240, 240);
const BAND: ((f64, f64, f64), (f64, f64, f64)) = (ORANGE_BTN_HSV_LO, ORANGE_BTN_HSV_HI);

fn paint(frame: &mut Frame, rect: (usize, usize, usize, usize), rgb: (u8, u8, u8)) {
    for y in rect.1..rect.3 {
        for x in rect.0..rect.2 {
            frame.set_rgb(x, y, rgb.0, rgb.1, rgb.2);
        }
    }
}

fn grey_frame() -> Frame {
    let mut frame = Frame::new(400, 300);
    frame.fill(GREY.0, GREY.1, GREY.2);
    frame
}

fn orange_button_frame() -> Frame {
    let mut frame = grey_frame();
    paint(&mut frame, (160, 200, 240, 226), ORANGE);
    frame
}

fn orange_at(rect: (usize, usize, usize, usize)) -> Frame {
    let mut frame = grey_frame();
    paint(&mut frame, rect, ORANGE);
    frame
}

fn repaint(frame: &Frame, pixels: usize) -> Frame {
    let mut frame = frame.clone();
    for index in 0..pixels {
        frame.set_rgb(index % 400, index / 400, 0, 0, 0);
    }
    frame
}

#[derive(Default)]
struct Recorder {
    actions: Vec<String>,
}

impl UiInput for Recorder {
    fn click(&mut self, _hwnd: isize, x: i32, y: i32) -> Result<()> {
        self.actions.push(format!("click {x},{y}"));
        Ok(())
    }

    fn press_enter(&mut self, _hwnd: isize) -> Result<()> {
        self.actions.push("enter".to_string());
        Ok(())
    }
}

#[test]
fn hsv_separates_orange_from_red_and_grey() {
    let (hue, saturation, value) = rgb_to_hsv(255, 140, 0);
    assert!((hue - 32.94).abs() < 0.1, "hue was {hue}");
    assert!((saturation - 1.0).abs() < 1e-9 && (value - 1.0).abs() < 1e-9);
    assert_eq!(rgb_to_hsv(255, 0, 0).0, 0.0);
    assert_eq!(rgb_to_hsv(128, 128, 128).1, 0.0);
    // The band is checked through the mask, so grey and pure red stay out of it.
    assert_eq!(blobs(&orange_at((10, 10, 11, 11)), BAND.0, BAND.1).len(), 1);
    let blue_band = ((200.0, 0.45, 0.55), (260.0, 1.0, 1.0));
    assert!(
        blobs(&grey_frame(), blue_band.0, blue_band.1).is_empty(),
        "grey carries no saturation"
    );
    assert!(blobs(&orange_at((10, 10, 11, 11)), blue_band.0, blue_band.1).is_empty());
    let mut red = grey_frame();
    paint(&mut red, (10, 10, 60, 40), (255, 0, 0));
    assert!(blobs(&red, BAND.0, BAND.1).is_empty(), "red is not orange");
}

#[test]
fn solid_orange_rectangle_is_one_blob_with_expected_bounds() {
    let blobs = blobs(&orange_button_frame(), BAND.0, BAND.1);
    assert_eq!(blobs.len(), 1);
    let blob = blobs[0];
    assert_eq!(
        (blob.left, blob.top, blob.right, blob.bottom),
        (160, 200, 239, 225)
    );
    assert_eq!(blob.area, 80 * 26);
    assert_eq!(blob.center(), (200, 213));
    assert!(blob.fill_ratio() > 0.99);
}

#[test]
fn separated_regions_are_distinct_blobs_and_red_or_grey_are_not_masked() {
    let mut frame = orange_button_frame();
    paint(&mut frame, (10, 10, 14, 14), (200, 40, 0)); // red-ish, outside the band
    frame.set_rgb(300, 260, ORANGE.0, ORANGE.1, ORANGE.2);
    let blobs = blobs(&frame, BAND.0, BAND.1);
    assert_eq!(blobs.len(), 2, "the stray button pixel is its own blob");
    let largest = blobs.iter().max_by_key(|blob| blob.area).expect("button");
    assert_eq!(largest.area, 80 * 26);
}

#[test]
fn uniform_detects_a_blank_render() {
    let mut frame = Frame::new(8, 4);
    frame.fill(10, 20, 30);
    assert!(frame.is_uniform());
    frame.set_rgb(3, 2, 10, 20, 31);
    assert!(!frame.is_uniform());
}

#[test]
fn diff_ratio_counts_changed_pixels_and_rejects_size_mismatch() {
    let left = orange_button_frame();
    assert_eq!(diff_ratio(&left, &left), Some(0.0));
    let right = repaint(&left, 20);
    let ratio = diff_ratio(&left, &right).expect("same size");
    assert!((ratio - 20.0 / 120_000.0).abs() < 1e-12);
    let noise = grey_frame(); // 240 vs 248: inside the tolerance
    let mut noise = noise;
    noise.set_rgb(0, 0, 248, 248, 248);
    assert_eq!(diff_ratio(&grey_frame(), &noise), Some(0.0));
    assert_eq!(diff_ratio(&left, &Frame::new(10, 10)), None);
}

#[test]
fn page_without_orange_evidence_sends_no_input() {
    assert_eq!(plan_for(&grey_frame()), PopupPlan::None);
    // A thin orange sliver is neither a button nor enough evidence.
    assert_eq!(plan_for(&orange_at((10, 10, 50, 14))), PopupPlan::None);
}

#[test]
fn scattered_thin_orange_slivers_never_trigger_the_enter_fallback() {
    // Live page (2026-09-15): 6331 orange pixels in 3220 thin slivers whose
    // largest blob was 18 px, so the page must plan no input at all.
    let mut frame = grey_frame();
    for index in 0..30 {
        let x = 10 + index * 12;
        paint(&mut frame, (x, 40, x + 1, 58), ORANGE);
    }
    assert_eq!(plan_for(&frame), PopupPlan::None);
}

#[test]
fn matched_orange_button_plans_the_nag_checkbox_before_confirm() {
    let plan = plan_for(&orange_button_frame());
    let PopupPlan::Click { nag, button } = plan else {
        panic!("expected a click plan, got {plan:?}");
    };
    assert_eq!(button, (200, 213));
    assert_eq!(nag, Some((200 + NAG_OFFSET_DX, 213 + NAG_OFFSET_DY)));
}

#[test]
fn nag_checkbox_is_skipped_when_its_offset_leaves_the_client_area() {
    let PopupPlan::Click { nag, button } = plan_for(&orange_at((370, 275, 400, 295))) else {
        panic!("expected a click plan");
    };
    assert_eq!(button, (385, 285));
    assert_eq!(nag, None, "the offset point is outside the client area");
}

#[test]
fn enter_fallback_requires_button_sized_orange_pixels() {
    // 60x60, aspect 1.0: not button geometry, but button-sized orange evidence.
    assert_eq!(plan_for(&orange_at((100, 100, 160, 160))), PopupPlan::Enter);
}

#[test]
fn execute_clicks_nag_then_confirm_and_never_more_than_one_enter() {
    let with_nag = PopupPlan::Click {
        nag: Some((350, 259)),
        button: (200, 213),
    };
    let mut recorder = Recorder::default();
    execute_plan(&mut recorder, 7, (100, 50), with_nag).expect("click plan");
    assert_eq!(recorder.actions, ["click 450,309", "click 300,263"]);

    let without_nag = PopupPlan::Click {
        nag: None,
        button: (200, 213),
    };
    let mut recorder = Recorder::default();
    execute_plan(&mut recorder, 7, (100, 50), without_nag).expect("click plan");
    assert_eq!(recorder.actions, ["click 300,263"]);

    let mut recorder = Recorder::default();
    execute_plan(&mut recorder, 7, (100, 50), PopupPlan::Enter).expect("enter plan");
    assert_eq!(recorder.actions, ["enter"]);

    let mut recorder = Recorder::default();
    execute_plan(&mut recorder, 7, (100, 50), PopupPlan::None).expect("no plan");
    assert!(recorder.actions.is_empty());
}

#[test]
fn verify_accepts_a_dismissed_and_stable_overlay() {
    let before = orange_button_frame();
    let settled = grey_frame();
    assert!(verify_dismissed(&before, &settled, &settled).is_ok());
    // Just below the 0.1 % limit (119 of 120000 pixels) still passes.
    assert!(verify_dismissed(&before, &repaint(&settled, 119), &settled).is_ok());
}

#[test]
fn verify_rejects_an_unchanged_client_area() {
    let before = orange_button_frame();
    let error = verify_dismissed(&before, &before, &before)
        .expect_err("an unchanged frame proves nothing")
        .to_string();
    assert!(error.contains("did not change"), "{error}");
}

#[test]
fn verify_rejects_a_repaint_above_the_diff_limit() {
    let before = orange_button_frame();
    let settled = grey_frame();
    for pixels in [120, 240] {
        let error = verify_dismissed(&before, &repaint(&settled, pixels), &settled)
            .expect_err("0.1 % and 0.2 % repaints must both fail")
            .to_string();
        assert!(error.contains("still changing"), "{error}");
    }
}

#[test]
fn verify_rejects_a_remaining_orange_template() {
    let before = orange_button_frame();
    let settled = orange_at((0, 0, 80, 26));
    let error = verify_dismissed(&before, &settled, &settled)
        .expect_err("a live orange button must fail")
        .to_string();
    assert!(error.contains("template still matched"), "{error}");
}
