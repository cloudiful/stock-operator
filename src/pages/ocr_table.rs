use anyhow::Result;
use axuielement::AXUIElement;

use super::{
    helpers::{
        element_geometry, sensitive_cell_indices, table_columns, table_matches_header_count,
        table_quality,
    },
    ocr::{OcrObservation, OcrSnapshot},
    types::{DataQuality, DataSource, ObservedText, PanelKind, TableColumn, TableRow},
};

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn overlap_ratio(self, other: Self) -> f64 {
        let left = self.x.max(other.x);
        let right = (self.x + self.width).min(other.x + other.width);
        let top = self.y.max(other.y);
        let bottom = (self.y + self.height).min(other.y + other.height);
        let overlap = (right - left).max(0.0) * (bottom - top).max(0.0);
        let area = self.width * self.height;
        if area <= 0.0 { 0.0 } else { overlap / area }
    }
}

pub(crate) struct OcrTableResult {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    pub quality: DataQuality,
    pub warnings: Vec<String>,
}

pub(crate) fn find_table<'a>(
    elements: &'a [AXUIElement],
    headers: &[&str],
    minimum_matches: usize,
) -> Option<&'a AXUIElement> {
    elements
        .iter()
        .find(|element| table_matches_header_count(element, headers, minimum_matches))
}

pub(crate) fn read_table(
    table: &AXUIElement,
    ocr: &OcrSnapshot,
    window: &AXUIElement,
    panel: PanelKind,
) -> Result<OcrTableResult> {
    let columns = table_columns(table);
    let window_rect = element_geometry(window).map(rect_from_tuple);
    let mut warnings = base_warnings(window_rect.is_none());
    warnings.extend(window_alignment_warnings(ocr, window_rect));
    if window_rect.is_none() {
        warnings.push(
            "target window geometry is unavailable; OCR cell matching was skipped".to_string(),
        );
    }
    let source_rows = table.element_array_attribute("AXRows").unwrap_or_default();
    let mut rows = Vec::new();
    let mut ambiguous_observations = 0;

    for row in source_rows {
        let cells = row.children().unwrap_or_default();
        let cell_rects = cells
            .iter()
            .map(|cell| element_geometry(cell).map(rect_from_tuple))
            .collect::<Vec<_>>();
        let sensitive_cells = sensitive_cell_indices(&columns, &cells);
        let values = match window_rect {
            Some(window_rect) => {
                let (values, ambiguous) =
                    match_cells(&cell_rects, ocr, window_rect, &sensitive_cells);
                ambiguous_observations += ambiguous;
                values
            }
            None => {
                vec![ObservedText::unavailable(); cells.len()]
            }
        };
        if values.iter().any(|value| value.source == DataSource::Ocr) {
            rows.push(TableRow {
                ordinal: rows.len(),
                cells: values,
            });
        }
    }
    if rows.is_empty() {
        warnings.push(no_rows_warning(panel));
    }
    if ambiguous_observations > 0 {
        warnings.push(format!(
            "{ambiguous_observations} OCR observation(s) overlapped multiple table cells and may contain merged columns"
        ));
    }
    let quality = table_quality(&rows);
    Ok(OcrTableResult {
        columns,
        rows,
        quality,
        warnings,
    })
}

fn base_warnings(geometry_unavailable: bool) -> Vec<String> {
    let mut warnings = vec![
        "values are read from Vision OCR and matched to Accessibility cell geometry; verify before use"
            .to_string(),
    ];
    if geometry_unavailable {
        warnings.push(
            "target window geometry is unavailable; OCR cell matching was skipped".to_string(),
        );
    }
    warnings
}

fn no_rows_warning(panel: PanelKind) -> String {
    match panel {
        PanelKind::Positions => "positions table exposed no rows".to_string(),
        PanelKind::Orders => "orders table exposed no rows".to_string(),
        PanelKind::Executions => "executions table exposed no rows".to_string(),
        PanelKind::Funds => "funds table exposed no rows".to_string(),
        _ => format!("{panel:?} table exposed no OCR-readable rows"),
    }
}

fn match_cells(
    cell_rects: &[Option<Rect>],
    ocr: &OcrSnapshot,
    window_rect: Rect,
    sensitive_cells: &std::collections::BTreeSet<usize>,
) -> (Vec<ObservedText>, usize) {
    let mut matches = vec![Vec::<&OcrObservation>::new(); cell_rects.len()];
    let mut ambiguous_observations = 0;
    for observation in &ocr.observations {
        let Some(rect) = ocr_rect(observation, ocr.width, ocr.height, window_rect) else {
            continue;
        };
        let overlapping = cell_rects
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| cell.map(|cell| (index, cell.overlap_ratio(rect))))
            .filter(|(_, overlap)| *overlap >= 0.1)
            .collect::<Vec<_>>();
        if overlapping.len() > 1 {
            ambiguous_observations += 1;
        }
        if let Some((index, _)) = overlapping
            .into_iter()
            .max_by(|left, right| left.1.total_cmp(&right.1))
        {
            matches[index].push(observation);
        }
    }
    let values = matches
        .into_iter()
        .enumerate()
        .map(|(index, observations)| {
            if sensitive_cells.contains(&index) {
                return if observations.is_empty() {
                    ObservedText::unavailable()
                } else {
                    ObservedText::redacted()
                };
            }
            let Some(first) = observations.first() else {
                return ObservedText::unavailable();
            };
            let text = observations
                .iter()
                .map(|observation| observation.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let confidence = observations
                .iter()
                .map(|observation| observation.confidence)
                .fold(first.confidence, f32::min);
            ObservedText::from_ocr(text, confidence)
        })
        .collect();
    (values, ambiguous_observations)
}

fn window_alignment_warnings(ocr: &OcrSnapshot, ax_window: Option<Rect>) -> Vec<String> {
    let Some(ax_window) = ax_window else {
        return Vec::new();
    };
    let width_ratio = if ax_window.width > 0.0 {
        ocr.window_width / ax_window.width
    } else {
        0.0
    };
    let height_ratio = if ax_window.height > 0.0 {
        ocr.window_height / ax_window.height
    } else {
        0.0
    };
    if width_ratio <= 0.0
        || height_ratio <= 0.0
        || (width_ratio / height_ratio - 1.0).abs() > 0.05
        || (ocr.window_x - ax_window.x).abs() > 4.0
        || (ocr.window_y - ax_window.y).abs() > 4.0
    {
        vec![
            "OCR captured window geometry does not match the AX window; cell mapping may be invalid"
                .to_string(),
        ]
    } else {
        Vec::new()
    }
}

fn rect_from_tuple((x, y, width, height): (f64, f64, f64, f64)) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn ocr_rect(observation: &OcrObservation, width: i64, height: i64, window: Rect) -> Option<Rect> {
    if width <= 0 || height <= 0 || window.width <= 0.0 || window.height <= 0.0 {
        return None;
    }
    Some(Rect {
        x: window.x + observation.x * window.width,
        y: window.y + (1.0 - observation.y - observation.height) * window.height,
        width: observation.width * window.width,
        height: observation.height * window.height,
    })
}

#[cfg(test)]
mod tests {
    use super::{base_warnings, no_rows_warning};
    use crate::pages::types::PanelKind;

    #[test]
    fn geometry_warning_is_added_once() {
        let warnings = base_warnings(true);
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.contains("OCR cell matching was skipped"))
                .count(),
            1
        );
    }

    #[test]
    fn panel_empty_warning_keeps_legacy_wording() {
        assert_eq!(
            no_rows_warning(PanelKind::Orders),
            "orders table exposed no rows"
        );
    }
}
