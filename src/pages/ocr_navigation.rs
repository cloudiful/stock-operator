use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use anyhow::{Result, bail};
use axuielement::AXUIElement;

use super::{
    helpers::{element_geometry, read_string},
    navigation::{NavigationCandidate, NavigationResult, NavigationTarget},
    ocr::OcrSnapshot,
    reader::PageReader,
    types::ViewDescriptor,
};

pub(crate) fn navigate(
    reader: &PageReader,
    target: NavigationTarget,
    before: ViewDescriptor,
) -> Result<NavigationResult> {
    let ocr = reader.ocr_visible_text()?;
    let Some(initial_candidate) = candidate(&ocr, target) else {
        bail!("expected exactly one verified OCR navigation candidate for {target:?}");
    };
    reader.focus_target_window()?;
    let focused_ocr = reader.ocr_visible_text()?;
    let Some(focused_candidate) = candidate(&focused_ocr, target) else {
        bail!("OCR navigation candidate disappeared after focusing the target window");
    };
    if initial_candidate.label != focused_candidate.label
        || (initial_candidate.x - focused_candidate.x).abs() > 0.01
        || (initial_candidate.y - focused_candidate.y).abs() > 0.01
    {
        bail!("OCR navigation candidate changed after focusing the target window");
    }
    press(reader, &focused_ocr, &focused_candidate)?;
    let after = verify(reader, target)?;
    let verified = after.panel == target.panel();
    Ok(NavigationResult {
        target,
        before,
        after,
        candidate: focused_candidate.as_public(),
        verified,
        warnings: if verified {
            vec!["navigation used OCR label matching".to_string()]
        } else {
            vec![format!("OCR navigation did not reach {:?}", target.panel())]
        },
    })
}

pub(crate) fn click_exact_label(
    reader: &PageReader,
    label: &str,
    y_range: std::ops::Range<f64>,
) -> Result<()> {
    let initial = reader.ocr_visible_text()?;
    let initial = exact_label_candidate(&initial, label, &y_range)
        .ok_or_else(|| anyhow::anyhow!("expected exactly one OCR label: {label}"))?;
    reader.focus_target_window()?;
    let focused = reader.ocr_visible_text()?;
    let focused = exact_label_candidate(&focused, label, &y_range)
        .ok_or_else(|| anyhow::anyhow!("OCR label disappeared after focus: {label}"))?;
    if (initial.0 - focused.0).abs() > 0.01 || (initial.1 - focused.1).abs() > 0.01 {
        bail!("OCR label moved after focus: {label}");
    }
    press_point(reader, &focused.2, focused.0, focused.1)
}

pub(crate) fn click_selected_security(reader: &PageReader, label: &str) -> Result<()> {
    let mut parts = label.split_whitespace();
    let Some(code) = parts.next() else {
        bail!("selected security label is empty");
    };
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) || parts.next().is_none()
    {
        bail!("selected security label must contain a six-digit code and name");
    }
    let initial = reader.ocr_visible_text()?;
    let initial = selected_security_candidate(&initial, label)
        .ok_or_else(|| anyhow::anyhow!("selected security label is not uniquely verified"))?;
    reader.focus_target_window()?;
    let focused = reader.ocr_visible_text()?;
    let focused = selected_security_candidate(&focused, label)
        .ok_or_else(|| anyhow::anyhow!("selected security label changed after focus"))?;
    if (initial.0 - focused.0).abs() > 0.005 || (initial.1 - focused.1).abs() > 0.005 {
        bail!("selected security label moved after focus");
    }
    press_point(reader, &focused.2, focused.0, focused.1)
}

fn selected_security_candidate(
    snapshot: &OcrSnapshot,
    label: &str,
) -> Option<(f64, f64, OcrSnapshot)> {
    let matches = snapshot
        .observations
        .iter()
        .filter(|observation| {
            observation.text.trim() == label && (0.84..0.90).contains(&observation.y)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return None;
    }
    let observation = matches[0];
    let has_field_label = snapshot.observations.iter().any(|field| {
        field.text.trim() == "证券"
            && field.x < observation.x
            && (field.y - observation.y).abs() <= 0.01
    });
    has_field_label.then(|| {
        (
            observation.x + observation.width / 2.0,
            observation.y + observation.height / 2.0,
            snapshot.clone(),
        )
    })
}

pub(crate) fn double_click_exact_label(
    reader: &PageReader,
    label: &str,
    y_range: std::ops::Range<f64>,
) -> Result<()> {
    let initial = reader.ocr_visible_text()?;
    let initial = exact_label_candidate(&initial, label, &y_range)
        .ok_or_else(|| anyhow::anyhow!("expected exactly one OCR label: {label}"))?;
    reader.focus_target_window()?;
    let focused = reader.ocr_visible_text()?;
    let focused = exact_label_candidate(&focused, label, &y_range)
        .ok_or_else(|| anyhow::anyhow!("OCR label disappeared after focus: {label}"))?;
    if (initial.0 - focused.0).abs() > 0.005 || (initial.1 - focused.1).abs() > 0.005 {
        bail!("OCR label moved after focus: {label}");
    }
    press_point_with(
        reader,
        &focused.2,
        focused.0,
        focused.1,
        super::mouse::double_click,
    )
}

fn exact_label_candidate(
    snapshot: &OcrSnapshot,
    label: &str,
    y_range: &std::ops::Range<f64>,
) -> Option<(f64, f64, OcrSnapshot)> {
    let mut matches = snapshot
        .observations
        .iter()
        .filter(|observation| {
            observation.confidence >= 0.9
                && observation.text.trim() == label
                && y_range.contains(&observation.y)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return None;
    }
    let observation = matches.pop()?;
    Some((
        observation.x + observation.width / 2.0,
        observation.y + observation.height / 2.0,
        snapshot.clone(),
    ))
}

#[derive(Clone, Debug)]
struct OcrCandidate {
    target: NavigationTarget,
    label: String,
    x: f64,
    y: f64,
}

impl OcrCandidate {
    fn as_public(&self) -> NavigationCandidate {
        NavigationCandidate {
            target: self.target,
            label: self.label.clone(),
            role: None,
            subrole: None,
            actions: vec!["OCRClick".to_string()],
            position: None,
            size: None,
        }
    }
}

fn candidate(snapshot: &OcrSnapshot, target: NavigationTarget) -> Option<OcrCandidate> {
    let allowed = NavigationTarget::ALL;
    let matches = snapshot
        .observations
        .iter()
        .filter(|observation| observation.confidence >= 0.9)
        .filter_map(|observation| {
            let target = allowed
                .iter()
                .find(|candidate| candidate.labels().contains(&observation.text.trim()))
                .copied()?;
            Some(OcrCandidate {
                target,
                label: observation.text.trim().to_string(),
                x: observation.x + observation.width / 2.0,
                y: observation.y + observation.height / 2.0,
            })
        })
        .collect::<Vec<_>>();
    let mut rows = BTreeMap::<i64, Vec<OcrCandidate>>::new();
    for candidate in matches {
        rows.entry((candidate.y * 100.0).round() as i64)
            .or_default()
            .push(candidate);
    }
    let rows = rows
        .into_values()
        .filter(|row| {
            row.iter()
                .map(|candidate| candidate.target)
                .collect::<BTreeSet<_>>()
                .len()
                >= 2
        })
        .collect::<Vec<_>>();
    if rows.len() != 1 {
        return None;
    }
    let mut matches = rows
        .into_iter()
        .next()?
        .into_iter()
        .filter(|candidate| candidate.target == target)
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches.pop().unwrap())
}

fn press(reader: &PageReader, snapshot: &OcrSnapshot, candidate: &OcrCandidate) -> Result<()> {
    press_point(reader, snapshot, candidate.x, candidate.y)
}

fn press_point(
    reader: &PageReader,
    snapshot: &OcrSnapshot,
    x_normalized: f64,
    y_normalized: f64,
) -> Result<()> {
    press_point_with(
        reader,
        snapshot,
        x_normalized,
        y_normalized,
        super::mouse::click,
    )
}

fn press_point_with(
    reader: &PageReader,
    snapshot: &OcrSnapshot,
    x_normalized: f64,
    y_normalized: f64,
    action: fn(f64, f64) -> Result<()>,
) -> Result<()> {
    if reader.target_status()?.target_pid != Some(snapshot.pid) {
        bail!("target process changed during OCR navigation");
    }
    let window = reader.focus_target_window()?;
    let Some((window_x, window_y, window_width, window_height)) = element_geometry(&window) else {
        bail!("target window geometry is unavailable");
    };
    if (window_x - snapshot.window_x).abs() > 4.0
        || (window_y - snapshot.window_y).abs() > 4.0
        || (window_width - snapshot.window_width).abs() > 4.0
        || (window_height - snapshot.window_height).abs() > 4.0
    {
        bail!("target window geometry changed during OCR navigation");
    }
    let x = window_x + window_width * x_normalized;
    let y = window_y + window_height * (1.0 - y_normalized);
    let Some(system) = AXUIElement::system_wide() else {
        bail!("system-wide Accessibility element is unavailable");
    };
    let Some(mut element) = system
        .element_at_position(x as f32, y as f32)
        .map_err(|error| anyhow::anyhow!("failed to resolve OCR navigation element: {error:?}"))?
    else {
        bail!("no Accessibility element was exposed at the OCR navigation label");
    };
    if element.pid().map_err(|error| {
        anyhow::anyhow!("failed to identify OCR navigation hit target: {error:?}")
    })? != snapshot.pid
    {
        bail!("OCR navigation point is covered by another application");
    }
    let mut inspected = Vec::new();
    for _ in 0..4 {
        let role = read_string(&element, "AXRole").unwrap_or_else(|| "unknown".to_string());
        let actions = element.action_names().unwrap_or_default();
        inspected.push(format!("{role}:{actions:?}"));
        if actions.iter().any(|action| action == "AXPress") {
            element.perform_action("AXPress").map_err(|error| {
                anyhow::anyhow!("failed to press OCR navigation element: {error:?}")
            })?;
            return Ok(());
        }
        let Ok(Some(parent)) = element.element_attribute("AXParent") else {
            break;
        };
        element = parent;
    }
    action(x, y).map_err(|error| {
        anyhow::anyhow!(
            "OCR navigation label was not AX-pressable ({}), and mouse click failed: {error}",
            inspected.join(" -> ")
        )
    })
}

fn verify(reader: &PageReader, target: NavigationTarget) -> Result<ViewDescriptor> {
    let mut after = reader.view()?;
    for _ in 0..8 {
        if after.panel == target.panel() {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
        after = reader.view()?;
    }
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::{NavigationTarget, candidate};
    use crate::pages::ocr::{OcrObservation, OcrSnapshot};

    #[test]
    fn finds_unique_target_in_verified_navigation_row() {
        let snapshot = snapshot(vec![
            observation("持仓", 1.0, 0.05),
            observation("当日委托", 1.0, 0.10),
            observation("资金明细", 1.0, 0.24),
        ]);
        assert_eq!(
            candidate(&snapshot, NavigationTarget::Funds).unwrap().label,
            "资金明细"
        );
    }

    #[test]
    fn rejects_low_confidence_or_isolated_labels() {
        assert!(
            candidate(
                &snapshot(vec![
                    observation("持仓", 1.0, 0.05),
                    observation("资金明细", 0.5, 0.24)
                ]),
                NavigationTarget::Funds
            )
            .is_none()
        );
        assert!(
            candidate(
                &snapshot(vec![observation("资金明细", 1.0, 0.24)]),
                NavigationTarget::Funds
            )
            .is_none()
        );
    }

    #[test]
    fn rejects_ambiguous_target() {
        let snapshot = snapshot(vec![
            observation("持仓", 1.0, 0.05),
            observation("资金明细", 1.0, 0.24),
            observation("资金明细", 1.0, 0.34),
        ]);
        assert!(candidate(&snapshot, NavigationTarget::Funds).is_none());
    }

    fn snapshot(observations: Vec<OcrObservation>) -> OcrSnapshot {
        OcrSnapshot {
            pid: 1,
            window_id: 2,
            window_x: 0.0,
            window_y: 0.0,
            window_width: 1000.0,
            window_height: 800.0,
            width: 2000,
            height: 1600,
            observations,
            warnings: Vec::new(),
        }
    }

    fn observation(text: &str, confidence: f32, x: f64) -> OcrObservation {
        OcrObservation {
            text: text.to_string(),
            confidence,
            x,
            y: 0.52,
            width: 0.03,
            height: 0.01,
        }
    }
}
