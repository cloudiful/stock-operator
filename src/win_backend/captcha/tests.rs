//! Unit tests for the copy-guard captcha solver (no live terminal needed).

use super::{
    detect::is_guard_text,
    ocr::{crop, normalize_code, upscale2},
    MAX_CODE_ATTEMPTS,
};
use crate::win_backend::popups::Frame;

#[test]
fn guard_text_gate_matches_only_the_copy_guard() {
    assert!(is_guard_text("检测到您正在拷贝数据，为保护您的账号数据安全，请先输入验证码："));
    assert!(!is_guard_text("委托下单(600018)"));
    assert!(!is_guard_text("提示"));
    assert!(!is_guard_text(""));
}

#[test]
fn spaced_digits_normalize_to_a_code() {
    assert_eq!(normalize_code("1 7 7 7"), Some("1777".to_string()));
    assert_eq!(normalize_code("1777"), Some("1777".to_string()));
    assert_eq!(normalize_code("  3057\n"), Some("3057".to_string()));
}

#[test]
fn non_four_digit_text_is_never_a_code() {
    assert_eq!(normalize_code("177"), None);
    assert_eq!(normalize_code("17777"), None);
    assert_eq!(normalize_code("abcd"), None);
    assert_eq!(normalize_code(""), None);
    assert_eq!(normalize_code("12 34 56"), None);
}

#[test]
fn crop_clamps_to_the_frame() {
    let frame = Frame::from_pixels(10, 10, vec![255u8; 10 * 10 * 4]).expect("frame");
    let inside = crop(&frame, 2, 2, 4, 4).expect("cropped");
    assert_eq!((inside.width, inside.height), (4, 4));
    let clamped = crop(&frame, 8, 8, 10, 10).expect("clamped");
    assert_eq!((clamped.width, clamped.height), (2, 2));
    assert!(crop(&frame, 20, 20, 5, 5).is_none());
    assert!(crop(&frame, 2, 2, 0, 4).is_none());
}

#[test]
fn upscale_preserves_corners() {
    let mut bgra = vec![0u8; 2 * 2 * 4];
    bgra[0..4].copy_from_slice(&[10, 20, 30, 255]);
    let frame = Frame::from_pixels(2, 2, bgra).expect("frame");
    let big = upscale2(&frame).expect("upscaled");
    assert_eq!((big.width, big.height), (4, 4));
    assert_eq!(big.rgb(0, 0), (30, 20, 10));
    assert_eq!(big.rgb(1, 1), (30, 20, 10));
}

#[test]
fn retry_budget_stays_small() {
    assert!(MAX_CODE_ATTEMPTS <= 3, "codes per call must stay bounded");
}
