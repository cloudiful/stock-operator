//! Pure parsing of `Static` window text read from the terminal. Everything here
//! operates on already-collected [`RawWindow`] values, so the layout heuristics
//! stay unit-testable without a live terminal.

use crate::backend::{FundsView, QuoteLevel, QuoteView, WinText};

use super::window::RawWindow;

/// Funds labels on the account page (login-state evidence 2026-09-15).
pub const FUNDS_LABELS: [&str; 5] = ["可用金额", "股票市值", "总资产", "冻结金额", "可取金额"];
/// Five-level quote labels in sell-then-buy display order.
pub const QUOTE_LEVEL_LABELS: [&str; 10] = [
    "卖五", "卖四", "卖三", "卖二", "卖一", "买一", "买二", "买三", "买四", "买五",
];
pub const LATEST_LABEL: &str = "最新";
pub const CHANGE_LABEL: &str = "涨幅";
pub const LIMIT_UP_LABEL: &str = "涨停";
pub const LIMIT_DOWN_LABEL: &str = "跌停";

const VERSION_LOW_MARKER: &str = "版本过低";

/// Placeholder shown before a code is entered: prices `888.888`, volumes
/// `88888888`. It is not a market value and must never be reported.
pub fn is_placeholder(value: &str) -> bool {
    let trimmed = value.trim();
    let digits = trimmed.strip_prefix('-').unwrap_or(trimmed);
    if digits.len() >= 3 && digits.chars().all(|c| c == '8') {
        return true;
    }
    matches!(parse_amount(trimmed), Some(amount) if (amount - 888.888).abs() < 1e-6)
}

/// Parses a displayed amount, tolerating grouping separators, currency marks and
/// `%`; the dash placeholder parses to `None`.
pub fn parse_amount(value: &str) -> Option<f64> {
    let cleaned: String = value
        .chars()
        .filter(|c| !matches!(c, ',' | ' ' | '\u{00a0}' | '¥' | '￥' | '+'))
        .collect();
    let cleaned = cleaned.trim_end_matches('%').trim();
    if cleaned.is_empty() || cleaned == "-" || cleaned == "--" {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

fn normalize_label(label: &str) -> String {
    label.chars().filter(|c| !c.is_whitespace()).collect()
}

fn looks_like_value(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed == "-" || trimmed == "--" || parse_amount(trimmed).is_some() || is_placeholder(trimmed)
}

/// Pairs every `Static` with the nearest label text to its left on the same row,
/// ordered geometrically (`top`, then `left`) so a label's price column precedes
/// its volume column; overlapping hidden pages alias labels and [`parse_funds`]
/// then prefers a numeric candidate.
pub fn pair_label_values(windows: &[RawWindow]) -> Vec<WinText> {
    let mut pairs: Vec<(&RawWindow, WinText)> = windows
        .iter()
        .filter(|window| window.class == "Static")
        .map(|window| {
            let label = if looks_like_value(&window.text) {
                nearest_label(windows, window)
                    .map(|label| normalize_label(&label.text))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let text = WinText {
                class: window.class.clone(),
                label,
                value: window.text.clone(),
                handle: window.handle,
            };
            (window, text)
        })
        .collect();
    pairs.sort_by_key(|(window, _)| (window.rect.top, window.rect.left));
    pairs.into_iter().map(|(_, text)| text).collect()
}

fn nearest_label<'a>(windows: &'a [RawWindow], value: &RawWindow) -> Option<&'a RawWindow> {
    windows
        .iter()
        .filter(|candidate| {
            candidate.class == "Static"
                && candidate.handle != value.handle
                && !candidate.text.trim().is_empty()
                && !looks_like_value(&candidate.text)
                && candidate.rect.left <= value.rect.left
                && candidate.rect.shares_row(value.rect)
        })
        .min_by_key(|candidate| {
            let overlap = std::cmp::Reverse(candidate.rect.vertical_overlap(value.rect));
            (overlap, (value.rect.left - candidate.rect.right).max(0))
        })
}

fn first_value<'a>(texts: &'a [WinText], label: &str) -> Option<&'a str> {
    let mut fallback = None;
    let mut numeric = None;
    for text in texts.iter().filter(|text| text.label == label) {
        if fallback.is_none() {
            fallback = Some(text.value.as_str());
        }
        if numeric.is_none() && parse_amount(&text.value).is_some() {
            numeric = Some(text.value.as_str());
        }
    }
    numeric.or(fallback)
}

pub fn parse_funds(texts: &[WinText]) -> FundsView {
    let mut placeholder = false;
    let mut amounts = [None; FUNDS_LABELS.len()];
    for (index, label) in FUNDS_LABELS.iter().enumerate() {
        let raw = first_value(texts, label);
        placeholder |= raw.map(is_placeholder).unwrap_or(false);
        amounts[index] = raw.filter(|value| !is_placeholder(value)).and_then(parse_amount);
    }
    let [available, market_value, total_assets, frozen, withdrawable] = amounts;
    let stale = available.is_none() || placeholder;
    FundsView { available, market_value, total_assets, frozen, withdrawable, stale }
}

pub fn parse_quotes(texts: &[WinText]) -> QuoteView {
    let mut levels = Vec::with_capacity(QUOTE_LEVEL_LABELS.len());
    let mut stale = false;
    for label in QUOTE_LEVEL_LABELS {
        let values: Vec<&WinText> = texts.iter().filter(|text| text.label == label).collect();
        let price_raw = values.first().map(|text| text.value.as_str());
        let volume_raw = values.get(1).map(|text| text.value.as_str());
        if price_raw.map(is_placeholder).unwrap_or(true) {
            stale = true;
        }
        let price = price_raw.filter(|value| !is_placeholder(value)).and_then(parse_amount);
        let volume = volume_raw
            .filter(|value| !is_placeholder(value))
            .and_then(parse_amount)
            .filter(|value| *value >= 0.0)
            .map(|value| value as u64);
        levels.push(QuoteLevel { label: label.to_string(), price, volume });
    }
    let scalar = |label: &str| {
        let raw = first_value(texts, label)?;
        (!is_placeholder(raw)).then(|| parse_amount(raw)).flatten()
    };
    let placeholder_scalar = [LATEST_LABEL, CHANGE_LABEL, LIMIT_UP_LABEL, LIMIT_DOWN_LABEL]
        .iter()
        .any(|label| first_value(texts, label).map(is_placeholder).unwrap_or(false));
    QuoteView {
        levels,
        latest: scalar(LATEST_LABEL),
        change_pct: scalar(CHANGE_LABEL),
        limit_up: scalar(LIMIT_UP_LABEL),
        limit_down: scalar(LIMIT_DOWN_LABEL),
        stale: stale || placeholder_scalar,
    }
}

/// True when a *visible* window contains the `版本过低` marker; the marker lives on
/// a `Static` inside a normally hidden `#32770` dialog, so a hidden hit must not
/// block mutations.
pub fn detect_version_low(windows: &[RawWindow]) -> bool {
    windows
        .iter()
        .any(|window| window.visible && window.text.contains(VERSION_LOW_MARKER))
}

/// Task 2 placeholder for the announcement overlay: the orange `确定` pixel
/// template lands in Task 3, so no overlay is reported as blocking here.
pub fn blocking_popup_detected() -> bool {
    false
}

pub const BLOCKING_POPUP_PLACEHOLDER_NOTE: &str =
    "blocking-popup detection is a Task 3 placeholder (no orange-button template yet): reported as not blocked";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win_backend::window::Rect;

    fn stat(handle: isize, left: i32, top: i32, text: &str, visible: bool) -> RawWindow {
        RawWindow {
            handle,
            parent: 1,
            depth: 2,
            class: "Static".to_string(),
            text: text.to_string(),
            visible,
            rect: Rect { left, top, right: left + 50, bottom: top + 12 },
        }
    }

    fn funds_fixture() -> Vec<RawWindow> {
        let rows = [
            (177, "可用金额", "12345.67"),
            (197, "冻结金额", "0.00"),
            (216, "股票市值", "24943.00"),
            (236, "总 资 产", "37288.67"),
            (255, "可取金额", "-"),
        ];
        rows.into_iter()
            .enumerate()
            .flat_map(|(index, (top, label, value))| {
                let base = 2 * index as isize;
                [stat(base + 1, 103, top, label, false), stat(base + 2, 157, top, value, false)]
            })
            .collect()
    }

    const LEVEL_ROWS: [i32; 10] = [223, 237, 250, 263, 277, 313, 327, 340, 353, 367];
    const LATEST_ROW: i32 = 295;

    fn quote_fixture() -> Vec<RawWindow> {
        let mut windows = Vec::new();
        for (index, label) in QUOTE_LEVEL_LABELS.iter().enumerate() {
            let top = LEVEL_ROWS[index];
            let handle = 100 + index as isize * 3;
            windows.push(stat(handle, 545, top, label, true));
            windows.push(stat(handle + 1, 570, top, "10.55", true));
            windows.push(stat(handle + 2, 696, top, "1200", true));
        }
        windows.push(stat(2000, 545, LATEST_ROW, LATEST_LABEL, true));
        windows.push(stat(2001, 570, LATEST_ROW, "10.55", true));
        windows.push(stat(2002, 656, LATEST_ROW, CHANGE_LABEL, true));
        windows.push(stat(2003, 696, LATEST_ROW, "0.35", true));
        windows
    }

    #[test]
    fn funds_fixture_parses_labels_and_numeric_shape() {
        let funds = parse_funds(&pair_label_values(&funds_fixture()));
        assert_eq!(funds.available, Some(12345.67));
        assert_eq!(funds.market_value, Some(24943.00));
        assert_eq!(funds.total_assets, Some(37288.67));
        assert_eq!(funds.frozen, Some(0.0));
        assert_eq!(funds.withdrawable, None);
        assert!(!funds.stale);
        assert!(funds.available.is_some_and(|value| value > 0.0));
        let aliased = [
            stat(1, 103, 216, "股票市值", false),
            stat(2, 157, 216, "-", false),
            stat(3, 157, 216, "24943.00", false),
        ];
        assert_eq!(
            parse_funds(&pair_label_values(&aliased)).market_value,
            Some(24943.00)
        );
    }

    #[test]
    fn placeholder_marks_funds_stale_without_a_value() {
        assert!(is_placeholder("888.888"));
        assert!(is_placeholder("88888888"));
        assert!(!is_placeholder("10.55"));
        assert_eq!(parse_amount("1,234.5"), Some(1234.5));
        assert_eq!(parse_amount("-"), None);
        let windows = [
            stat(1, 103, 177, "可用金额", true),
            stat(2, 157, 177, "888.888", true),
        ];
        let funds = parse_funds(&pair_label_values(&windows));
        assert!(funds.stale);
        assert_eq!(funds.available, None);
    }

    #[test]
    fn quotes_fixture_keeps_price_before_volume_and_flags_placeholders() {
        let quotes = parse_quotes(&pair_label_values(&quote_fixture()));
        assert_eq!(quotes.levels.len(), 10);
        assert_eq!(quotes.levels[0].label, "卖五");
        assert_eq!(quotes.levels[4].label, "卖一");
        assert_eq!(quotes.levels[4].price, Some(10.55));
        assert_eq!(quotes.levels[4].volume, Some(1200));
        assert_eq!(quotes.latest, Some(10.55));
        assert_eq!(quotes.change_pct, Some(0.35));
        assert!(!quotes.stale);
        let stale = parse_quotes(&pair_label_values(&[
            stat(1, 545, 277, "卖一", true),
            stat(2, 570, 277, "888.888", true),
            stat(3, 696, 277, "88888888", true),
        ]));
        assert_eq!(stale.levels[4].price, None);
        assert_eq!(stale.levels[4].volume, None);
        assert!(stale.stale);
    }

    #[test]
    fn version_low_marker_requires_a_visible_window() {
        let marker = "尊敬的用户：您当前版本过低，请重新下载安装。";
        assert!(!detect_version_low(&[stat(1, 313, 307, marker, false)]));
        assert!(detect_version_low(&[stat(1, 313, 307, marker, true)]));
    }
}
