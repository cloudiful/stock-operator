use std::collections::BTreeSet;

use axuielement::AXUIElement;

use super::types::{
    ControlDescriptor, DataQuality, ObservedText, PanelKind, TableColumn, TableDiagnosticElement,
    TableRow, WorkspaceKind,
};
use crate::ax::attribute_value_json;

pub(crate) fn collect_elements(root: &AXUIElement, limit: usize) -> Vec<AXUIElement> {
    let mut elements = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(element) = pending.pop() {
        if elements.len() >= limit {
            break;
        }
        if let Ok(children) = element.children() {
            pending.extend(children.into_iter().rev());
        }
        elements.push(element);
    }
    elements
}

pub(crate) fn detect_workspace(elements: &[AXUIElement]) -> WorkspaceKind {
    let text = visible_text(elements).join(" ");
    if text.contains("卖出下单") {
        WorkspaceKind::SellOrder
    } else if text.contains("买入下单") {
        WorkspaceKind::BuyOrder
    } else {
        WorkspaceKind::Unknown
    }
}

pub(crate) fn detect_panel(elements: &[AXUIElement]) -> PanelKind {
    let text = visible_text(elements).join(" ");
    if text.contains("全选中") && text.contains("全不选") {
        return PanelKind::Cancellations;
    }
    if elements.iter().any(|element| {
        table_matches_header_count(element, &["委托编号", "委托状态", "委托价格"], 2)
    }) {
        return PanelKind::Orders;
    }
    if elements.iter().any(|element| {
        table_matches_header_count(element, &["成交编号", "成交价格", "成交数量"], 2)
    }) {
        return PanelKind::Executions;
    }
    if elements
        .iter()
        .any(|element| table_matches_header_count(element, &["可用资金", "冻结资金", "总资产"], 2))
    {
        return PanelKind::Funds;
    }
    if elements.iter().any(is_positions_table) {
        return PanelKind::Positions;
    }
    if text.contains("当日成交") {
        PanelKind::Executions
    } else if text.contains("当日委托") {
        PanelKind::Orders
    } else if text.contains("资金明细") {
        PanelKind::Funds
    } else {
        PanelKind::Unknown
    }
}

pub(crate) fn page_evidence(
    elements: &[AXUIElement],
    workspace: WorkspaceKind,
    panel: PanelKind,
) -> Vec<String> {
    let mut evidence = BTreeSet::new();
    for value in visible_text(elements) {
        if matches!(
            workspace,
            WorkspaceKind::BuyOrder | WorkspaceKind::SellOrder
        ) && (value.contains("下单") || value == "全部")
        {
            evidence.insert(value.clone());
        }
        if matches!(panel, PanelKind::Positions) && value.contains("证券") {
            evidence.insert(value);
        }
    }
    evidence.into_iter().collect()
}

pub(crate) fn is_interactive(element: &AXUIElement) -> bool {
    matches!(
        read_string(element, "AXRole").as_deref(),
        Some("AXButton")
            | Some("AXTextField")
            | Some("AXComboBox")
            | Some("AXPopUpButton")
            | Some("AXMenuButton")
            | Some("AXRadioButton")
            | Some("AXCheckBox")
    )
}

pub(crate) fn control_descriptor(element: AXUIElement) -> ControlDescriptor {
    ControlDescriptor {
        role: read_string(&element, "AXRole"),
        subrole: read_string(&element, "AXSubrole"),
        title: read_string(&element, "AXTitle"),
        identifier: read_string(&element, "AXIdentifier"),
        description: read_string(&element, "AXDescription"),
        value: ObservedText::from_value(read_text(&element)),
        actions: element.action_names().unwrap_or_default(),
        position: read_json_value(&element, "AXPosition"),
        size: read_json_value(&element, "AXSize"),
    }
}

pub(crate) fn is_positions_table(element: &AXUIElement) -> bool {
    if read_string(element, "AXRole").as_deref() != Some("AXTable") {
        return false;
    }
    table_matches_header_count(
        element,
        &[
            "证券代码",
            "证券名称",
            "昨日余额",
            "参考持股",
            "可用股份",
            "成本价",
        ],
        4,
    )
}

pub(crate) fn is_table(element: &AXUIElement) -> bool {
    read_string(element, "AXRole").as_deref() == Some("AXTable")
}

pub(crate) fn table_matches_header_count(
    element: &AXUIElement,
    keywords: &[&str],
    minimum_matches: usize,
) -> bool {
    if !is_table(element) {
        return false;
    }
    let headers = table_columns(element)
        .into_iter()
        .filter_map(|column| column.title.value)
        .collect::<Vec<_>>();
    keywords
        .iter()
        .filter(|keyword| headers.iter().any(|header| header.contains(**keyword)))
        .count()
        >= minimum_matches
}

pub(crate) fn table_quality(rows: &[TableRow]) -> DataQuality {
    if rows.is_empty() {
        return DataQuality::Unavailable;
    }
    let mut has_exact = false;
    let mut has_unavailable = false;
    for cell in rows.iter().flat_map(|row| row.cells.iter()) {
        has_exact |= cell.quality == DataQuality::Exact;
        has_unavailable |= cell.quality == DataQuality::Unavailable;
    }
    if !has_exact {
        DataQuality::Unavailable
    } else if has_unavailable {
        DataQuality::Partial
    } else {
        DataQuality::Exact
    }
}

pub(crate) fn table_columns(table: &AXUIElement) -> Vec<TableColumn> {
    table
        .element_array_attribute("AXColumns")
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(ordinal, column)| TableColumn {
            ordinal,
            identifier: read_string(&column, "AXIdentifier"),
            title: column
                .element_attribute("AXHeader")
                .ok()
                .flatten()
                .map(|header| ObservedText::from_value(read_text(&header)))
                .unwrap_or_else(ObservedText::unavailable),
            position: read_json_value(&column, "AXPosition"),
            size: read_json_value(&column, "AXSize"),
        })
        .collect()
}

pub(crate) fn sensitive_cell_indices(
    columns: &[TableColumn],
    cells: &[AXUIElement],
) -> std::collections::BTreeSet<usize> {
    let sensitive_columns = columns
        .iter()
        .filter(|column| {
            column
                .title
                .value
                .as_deref()
                .is_some_and(|title| matches!(title, "股东代码" | "托管单元" | "资金账号"))
        })
        .collect::<Vec<_>>();
    if sensitive_columns.is_empty() {
        return std::collections::BTreeSet::new();
    }
    let column_geometry = sensitive_columns
        .iter()
        .map(|column| table_column_geometry(column))
        .collect::<Vec<_>>();
    if column_geometry.iter().any(Option::is_none) {
        return (0..cells.len()).collect();
    }
    cells
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| {
            let Some(cell_geometry) = element_geometry(cell) else {
                return Some((index, true));
            };
            let sensitive = sensitive_columns.iter().any(|column| {
                let column_geometry = table_column_geometry(column).expect("validated above");
                horizontal_overlap(cell_geometry, column_geometry) >= 0.25
            });
            Some((index, sensitive))
        })
        .filter_map(|(index, sensitive)| sensitive.then_some(index))
        .collect()
}

fn table_column_geometry(column: &TableColumn) -> Option<(f64, f64, f64, f64)> {
    let position = column.position.as_ref()?.as_object()?;
    let size = column.size.as_ref()?.as_object()?;
    Some((
        position.get("x")?.as_f64()?,
        position.get("y")?.as_f64()?,
        size.get("width")?.as_f64()?,
        size.get("height")?.as_f64()?,
    ))
}

pub(crate) fn element_geometry(element: &AXUIElement) -> Option<(f64, f64, f64, f64)> {
    let position = element.point_attribute("AXPosition").ok().flatten()?;
    let size = element.size_attribute("AXSize").ok().flatten()?;
    Some((position.x, position.y, size.width, size.height))
}

fn horizontal_overlap(left: (f64, f64, f64, f64), right: (f64, f64, f64, f64)) -> f64 {
    let left_edge = left.0.max(right.0);
    let right_edge = (left.0 + left.2).min(right.0 + right.2);
    let overlap = (right_edge - left_edge).max(0.0);
    let width = left.2.min(right.2);
    if width <= 0.0 { 0.0 } else { overlap / width }
}

pub(crate) fn table_rows(table: &AXUIElement, limit: usize) -> Vec<TableRow> {
    table
        .element_array_attribute("AXRows")
        .unwrap_or_default()
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(ordinal, row)| TableRow {
            ordinal,
            cells: row
                .children()
                .unwrap_or_default()
                .into_iter()
                .map(|cell| ObservedText::from_value(read_text(&cell)))
                .collect(),
        })
        .collect()
}

pub(crate) fn diagnostic_element(
    element: &AXUIElement,
    depth: usize,
    max_depth: usize,
    max_children: usize,
    redacted: bool,
) -> TableDiagnosticElement {
    let attributes = element.attribute_names().unwrap_or_default();
    let values = if redacted {
        Default::default()
    } else {
        attributes
            .iter()
            .filter(|attribute| !structural_attribute(attribute))
            .filter_map(|attribute| {
                attribute_value_json(element, attribute).map(|value| (attribute.clone(), value))
            })
            .take(64)
            .collect()
    };
    let source_children = if depth < max_depth {
        element.children().unwrap_or_default()
    } else {
        Vec::new()
    };
    let child_count = source_children.len();
    let children = source_children
        .into_iter()
        .take(max_children)
        .map(|child| diagnostic_element(&child, depth + 1, max_depth, max_children, redacted))
        .collect();

    TableDiagnosticElement {
        role: read_string(element, "AXRole"),
        subrole: read_string(element, "AXSubrole"),
        title: read_string(element, "AXTitle"),
        identifier: read_string(element, "AXIdentifier"),
        description: read_string(element, "AXDescription"),
        values,
        attributes,
        actions: element.action_names().unwrap_or_default(),
        child_count,
        children,
        redacted,
    }
}

fn structural_attribute(attribute: &str) -> bool {
    matches!(
        attribute,
        "AXChildren"
            | "AXParent"
            | "AXWindow"
            | "AXTopLevelUIElement"
            | "AXTitleUIElement"
            | "AXFocusedUIElement"
            | "AXSelectedChildren"
            | "AXVisibleChildren"
            | "AXRows"
            | "AXColumns"
    )
}

const MAX_TEXT_DEPTH: usize = 4;

pub(crate) fn read_text(element: &AXUIElement) -> Option<String> {
    read_text_at_depth(element, 0)
}

fn read_text_at_depth(element: &AXUIElement, depth: usize) -> Option<String> {
    read_string(element, "AXValue")
        .or_else(|| read_string(element, "AXTitle"))
        .or_else(|| read_string(element, "AXDescription"))
        .or_else(|| {
            if depth >= MAX_TEXT_DEPTH {
                return None;
            }
            element
                .element_attribute("AXTitleUIElement")
                .ok()
                .flatten()
                .and_then(|title| read_text_at_depth(&title, depth + 1))
        })
        .or_else(|| {
            if depth >= MAX_TEXT_DEPTH {
                return None;
            }
            element
                .children()
                .ok()?
                .into_iter()
                .map(|child| read_text_at_depth(&child, depth + 1))
                .flatten()
                .next()
        })
}

pub(crate) fn read_string(element: &AXUIElement, attribute: &str) -> Option<String> {
    element.string_attribute(attribute).ok().flatten()
}

pub(crate) fn read_bool(element: &AXUIElement, attribute: &str) -> Option<bool> {
    element.bool_attribute(attribute).ok().flatten()
}

pub(crate) fn read_json_value(element: &AXUIElement, attribute: &str) -> Option<serde_json::Value> {
    let value = element.attribute(attribute).ok().flatten()?;
    match value.kind() {
        axuielement::AXValueKind::Point => value.as_point().map(|point| {
            serde_json::json!({
                "x": point.x,
                "y": point.y,
            })
        }),
        axuielement::AXValueKind::Size => value.as_size().map(|size| {
            serde_json::json!({
                "width": size.width,
                "height": size.height,
            })
        }),
        axuielement::AXValueKind::Rect => value.as_rect().map(|rect| {
            serde_json::json!({
                "origin": { "x": rect.origin.x, "y": rect.origin.y },
                "size": { "width": rect.size.width, "height": rect.size.height },
            })
        }),
        _ => None,
    }
}

fn visible_text(elements: &[AXUIElement]) -> Vec<String> {
    elements
        .iter()
        .flat_map(|element| {
            [
                read_string(element, "AXTitle"),
                read_string(element, "AXDescription"),
                read_string(element, "AXValue"),
            ]
        })
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect()
}

#[allow(dead_code)]
fn _quality_of_texts(values: &[ObservedText]) -> DataQuality {
    if values.is_empty() {
        DataQuality::Unavailable
    } else if values
        .iter()
        .all(|value| value.quality == DataQuality::Exact)
    {
        DataQuality::Exact
    } else {
        DataQuality::Partial
    }
}
