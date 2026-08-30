use anyhow::Result;
use schemars::JsonSchema;

use crate::ax::AccessibilityInspector;

use super::{
    helpers::{
        collect_elements, control_descriptor, detect_panel, detect_workspace, diagnostic_element,
        is_interactive, is_positions_table, page_evidence, read_bool, read_json_value, read_string,
        read_text, sensitive_cell_indices, table_columns, table_rows,
    },
    types::{
        DataQuality, PageSnapshot, PanelKind, PositionsSnapshot, PositionsTableDiagnostic,
        TableDiagnosticElement, TradeFormControl, TradeFormSnapshot, UiInventory, ViewDescriptor,
        WorkspaceKind, empty_snapshot, snapshot,
    },
};

#[derive(Clone)]
pub struct PageReader {
    inspector: AccessibilityInspector,
}

impl PageReader {
    pub fn new(inspector: AccessibilityInspector) -> Self {
        Self { inspector }
    }

    pub(crate) fn current_elements(
        &self,
        limit: usize,
    ) -> Result<Option<Vec<axuielement::AXUIElement>>> {
        let Some(window) = self.inspector.target_window()? else {
            return Ok(None);
        };
        Ok(Some(collect_elements(&window, limit)))
    }

    pub(crate) fn current_window(&self) -> Result<Option<axuielement::AXUIElement>> {
        self.inspector.target_window()
    }

    pub(crate) fn focus_target_window(&self) -> Result<axuielement::AXUIElement> {
        self.inspector.focus_target_window()
    }

    pub(crate) fn target_status(&self) -> Result<crate::ax::AccessibilityStatus> {
        Ok(self.inspector.status())
    }

    pub fn inventory(&self, max_controls: usize) -> Result<UiInventory> {
        let Some(window) = self.inspector.target_window()? else {
            return Ok(UiInventory {
                workspace: WorkspaceKind::Unknown,
                panel: PanelKind::Unknown,
                controls: Vec::new(),
                warnings: vec!["target window is unavailable".to_string()],
            });
        };
        let elements = collect_elements(&window, max_controls.saturating_mul(4).max(1));
        let workspace = detect_workspace(&elements);
        let panel = detect_panel(&elements);
        let controls = elements
            .into_iter()
            .filter(is_interactive)
            .take(max_controls)
            .map(control_descriptor)
            .collect();
        Ok(UiInventory {
            workspace,
            panel,
            controls,
            warnings: Vec::new(),
        })
    }

    pub fn view(&self) -> Result<ViewDescriptor> {
        let Some(window) = self.inspector.target_window()? else {
            return Ok(ViewDescriptor {
                workspace: WorkspaceKind::Unknown,
                panel: PanelKind::Unknown,
                evidence: Vec::new(),
                warnings: vec!["target window is unavailable".to_string()],
            });
        };
        let elements = collect_elements(&window, 1_000);
        let workspace = detect_workspace(&elements);
        let panel = detect_panel(&elements);
        Ok(ViewDescriptor {
            workspace,
            panel,
            evidence: page_evidence(&elements, workspace, panel),
            warnings: Vec::new(),
        })
    }

    pub fn trade_form(&self) -> Result<PageSnapshot<TradeFormSnapshot>> {
        let Some(window) = self.inspector.target_window()? else {
            return Ok(empty_snapshot(
                WorkspaceKind::Unknown,
                PanelKind::Unknown,
                TradeFormSnapshot {
                    controls: Vec::new(),
                },
                vec!["target window is unavailable".to_string()],
            ));
        };
        let elements = collect_elements(&window, 1_000);
        let workspace = detect_workspace(&elements);
        let panel = detect_panel(&elements);
        if !matches!(
            workspace,
            WorkspaceKind::BuyOrder | WorkspaceKind::SellOrder
        ) {
            return Ok(empty_snapshot(
                workspace,
                panel,
                TradeFormSnapshot {
                    controls: Vec::new(),
                },
                vec!["current workspace is not a buy or sell order form".to_string()],
            ));
        }
        let controls = elements
            .into_iter()
            .filter(|element| {
                matches!(
                    read_string(element, "AXRole").as_deref(),
                    Some("AXTextField")
                ) && element
                    .action_names()
                    .unwrap_or_default()
                    .iter()
                    .any(|action| action == "AXConfirm")
            })
            .enumerate()
            .map(|(ordinal, element)| TradeFormControl {
                ordinal,
                role: "AXTextField".to_string(),
                value: super::types::ObservedText::from_value(read_text(&element)),
                focused: read_bool(&element, "AXFocused"),
                position: read_json_value(&element, "AXPosition"),
                actions: element.action_names().unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        let mut warnings = Vec::new();
        if controls.is_empty() {
            warnings.push(
                "no trade form AXTextField with AXConfirm was exposed by the current window"
                    .to_string(),
            );
        }
        if controls
            .iter()
            .any(|control| control.value.quality == DataQuality::Unavailable)
        {
            warnings.push(
                "one or more trade form values are unavailable via Accessibility".to_string(),
            );
        }
        let quality = quality_of_controls(&controls);
        Ok(snapshot(
            workspace,
            panel,
            TradeFormSnapshot { controls },
            quality,
            warnings,
        ))
    }

    pub fn positions(&self) -> Result<PageSnapshot<PositionsSnapshot>> {
        let Some(window) = self.inspector.target_window()? else {
            return Ok(empty_snapshot(
                WorkspaceKind::Unknown,
                PanelKind::Unknown,
                PositionsSnapshot {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    row_count: 0,
                },
                vec!["target window is unavailable".to_string()],
            ));
        };
        let elements = collect_elements(&window, 2_000);
        let workspace = detect_workspace(&elements);
        let table = elements.iter().find(|element| is_positions_table(element));
        let Some(table) = table else {
            return Ok(empty_snapshot(
                workspace,
                PanelKind::Unknown,
                PositionsSnapshot {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    row_count: 0,
                },
                vec!["positions table was not exposed by the current window".to_string()],
            ));
        };

        let columns = table_columns(table);
        let rows = table_rows(table, 500);
        let quality = quality_of_rows(&rows);
        let mut warnings = Vec::new();
        if quality != DataQuality::Exact {
            warnings.push(
                "table structure is readable, but one or more cell values are unavailable via Accessibility"
                    .to_string(),
            );
        }
        Ok(snapshot(
            workspace,
            PanelKind::Positions,
            PositionsSnapshot {
                row_count: rows.len(),
                columns,
                rows,
            },
            quality,
            warnings,
        ))
    }

    pub fn diagnose_positions_table(
        &self,
        max_rows: usize,
        max_cells: usize,
        max_depth: usize,
    ) -> Result<PositionsTableDiagnostic> {
        let Some(window) = self.inspector.target_window()? else {
            return Ok(PositionsTableDiagnostic {
                table: None,
                rows: Vec::new(),
                exposed_row_count: 0,
                inspected_row_count: 0,
                warnings: vec!["target window is unavailable".to_string()],
            });
        };
        let elements = collect_elements(&window, 2_000);
        let Some(table) = elements.iter().find(|element| is_positions_table(element)) else {
            return Ok(PositionsTableDiagnostic {
                table: None,
                rows: Vec::new(),
                exposed_row_count: 0,
                inspected_row_count: 0,
                warnings: vec!["positions table was not exposed by the current window".to_string()],
            });
        };

        let columns = table_columns(table);
        let source_rows = table.element_array_attribute("AXRows").unwrap_or_default();
        let exposed_row_count = source_rows.len();
        let rows = source_rows
            .into_iter()
            .take(max_rows)
            .map(|row| {
                let cells = row.children().unwrap_or_default();
                let sensitive_cells = sensitive_cell_indices(&columns, &cells);
                let child_count = cells.len();
                let children = cells
                    .into_iter()
                    .take(max_cells)
                    .enumerate()
                    .map(|(cell_index, cell)| {
                        diagnostic_element(
                            &cell,
                            0,
                            max_depth,
                            8,
                            sensitive_cells.contains(&cell_index),
                        )
                    })
                    .collect::<Vec<_>>();
                TableDiagnosticElement {
                    role: read_string(&row, "AXRole"),
                    subrole: read_string(&row, "AXSubrole"),
                    title: read_string(&row, "AXTitle"),
                    identifier: read_string(&row, "AXIdentifier"),
                    description: read_string(&row, "AXDescription"),
                    values: std::collections::BTreeMap::new(),
                    attributes: row.attribute_names().unwrap_or_default(),
                    actions: row.action_names().unwrap_or_default(),
                    child_count,
                    children,
                    redacted: false,
                }
            })
            .collect::<Vec<_>>();

        let mut warnings = Vec::new();
        if exposed_row_count > max_rows {
            warnings.push("diagnostic row limit truncated the exposed rows".to_string());
        }
        if rows.iter().any(|row| row.child_count > max_cells) {
            warnings.push("diagnostic cell limit truncated one or more rows".to_string());
        }
        let inspected_row_count = rows.len();
        Ok(PositionsTableDiagnostic {
            table: Some(diagnostic_element(table, 0, 0, 0, false)),
            rows,
            exposed_row_count,
            inspected_row_count,
            warnings,
        })
    }
}

fn quality_of_controls(controls: &[TradeFormControl]) -> DataQuality {
    if controls.is_empty() {
        DataQuality::Unavailable
    } else if controls
        .iter()
        .all(|control| control.value.quality == DataQuality::Exact)
    {
        DataQuality::Exact
    } else {
        DataQuality::Partial
    }
}

fn quality_of_rows(rows: &[super::types::TableRow]) -> DataQuality {
    if rows.is_empty() {
        DataQuality::Unavailable
    } else {
        let cells = rows.iter().flat_map(|row| row.cells.iter());
        let mut has_exact = false;
        let mut has_unavailable = false;
        for cell in cells {
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
}

#[allow(dead_code)]
fn _assert_schema<T: JsonSchema>() {}
