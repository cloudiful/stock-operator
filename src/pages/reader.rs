use std::sync::Arc;

use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use serde::Serialize;

use crate::backend::Backend;
use crate::win_backend::{GridPanel, GridTable, grid, window};

use super::{
    cancel_order::{CancelNavigationResult, CancellationTarget},
    navigation::{NavigationCandidates, NavigationResult, NavigationTarget},
    ocr::OcrSnapshot,
    stage_order::{StageOrderRequest, StageOrderResult},
    trade_preflight::{TradePreflightRequest, TradePreflightResult},
    types::{
        DataQuality, ExecutionsSnapshot, FundsSnapshot, ObservedText, OrdersSnapshot, PageSnapshot,
        PanelKind, PositionsSnapshot, PositionsTableDiagnostic, TableColumn, TableRow,
        TradeFormSnapshot, UiInventory, ViewDescriptor, WorkspaceKind, snapshot,
    },
};

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct SubmitOrderResult {
    pub status: String,
    pub request: StageOrderRequest,
    pub after: ViewDescriptor,
    pub visible_prompts: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct SecuritySelectionResult {
    pub security_code: String,
    pub selected_text: String,
    pub derived_price: String,
}

#[derive(Clone)]
pub struct PageReader {
    #[allow(dead_code)]
    backend: Arc<dyn Backend>,
}

/// A grid table converted to the page-table shape used by the read snapshots.
pub(crate) struct ObservedGridTable {
    pub(crate) columns: Vec<TableColumn>,
    pub(crate) rows: Vec<TableRow>,
    pub(crate) quality: DataQuality,
    pub(crate) warnings: Vec<String>,
}

impl ObservedGridTable {
    pub(crate) fn row_count(&self) -> usize {
        self.rows.len()
    }
}

impl PageReader {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }

    /// Reads `panel` from the grid the terminal is currently displaying.
    ///
    /// The grid is owner-drawn, so the table is copied with `Tab` focus plus
    /// `Ctrl+A`/`Ctrl+C` and parsed from the clipboard TSV (`win_backend::grid`).
    /// Clicks, `Enter`, panel shortcuts and order controls are never sent, so a
    /// panel that is not open fails loudly instead of returning another table.
    pub(crate) fn grid_observed_table(&self, panel: GridPanel) -> Result<ObservedGridTable> {
        let main_hwnd = window::find_main_window()?;
        let table = grid::read_grid(main_hwnd, panel)?;
        Ok(observed_grid_table(panel, &table))
    }

    pub fn inventory(&self, _max_controls: usize) -> Result<UiInventory> {
        stub()
    }

    pub fn view(&self) -> Result<ViewDescriptor> {
        stub()
    }

    pub fn trade_form(&self) -> Result<PageSnapshot<TradeFormSnapshot>> {
        stub()
    }

    pub fn positions(&self) -> Result<PageSnapshot<PositionsSnapshot>> {
        let table = self.grid_observed_table(GridPanel::Positions)?;
        Ok(snapshot(
            WorkspaceKind::Unknown,
            PanelKind::Positions,
            PositionsSnapshot {
                row_count: table.row_count(),
                columns: table.columns,
                rows: table.rows,
            },
            table.quality,
            table.warnings,
        ))
    }

    pub fn diagnose_positions_table(
        &self,
        _max_rows: usize,
        _max_cells: usize,
        _max_depth: usize,
    ) -> Result<PositionsTableDiagnostic> {
        stub()
    }

    pub fn navigation_candidates(&self) -> Result<NavigationCandidates> {
        stub()
    }

    pub fn navigate_readonly(&self, _target: NavigationTarget) -> Result<NavigationResult> {
        stub()
    }

    pub fn navigate_to_cancellations(&self) -> Result<CancelNavigationResult> {
        stub()
    }

    pub fn open_cancel_confirmation(&self, _target: &CancellationTarget) -> Result<()> {
        stub()
    }

    pub fn close_cancel_selection_warning(&self) -> Result<()> {
        stub()
    }

    pub fn confirm_cancel_order(&self, _target: &CancellationTarget) -> Result<()> {
        stub()
    }

    pub fn close_cancel_submitted_notice(&self) -> Result<()> {
        stub()
    }

    pub fn cancel_cancellation_confirmation(&self, _target: &CancellationTarget) -> Result<()> {
        stub()
    }

    pub fn ocr_visible_text(&self) -> Result<OcrSnapshot> {
        stub()
    }

    pub fn cancellations(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        stub()
    }

    pub fn orders(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        let table = self.grid_observed_table(GridPanel::Orders)?;
        Ok(snapshot(
            WorkspaceKind::Unknown,
            PanelKind::Orders,
            OrdersSnapshot {
                row_count: table.row_count(),
                columns: table.columns,
                rows: table.rows,
            },
            table.quality,
            table.warnings,
        ))
    }

    pub fn executions(&self) -> Result<PageSnapshot<ExecutionsSnapshot>> {
        let table = self.grid_observed_table(GridPanel::Executions)?;
        Ok(snapshot(
            WorkspaceKind::Unknown,
            PanelKind::Executions,
            ExecutionsSnapshot {
                row_count: table.row_count(),
                columns: table.columns,
                rows: table.rows,
            },
            table.quality,
            table.warnings,
        ))
    }

    pub fn funds(&self) -> Result<PageSnapshot<FundsSnapshot>> {
        stub()
    }

    pub fn cancellations_ocr(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        stub()
    }

    pub fn orders_ocr(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        stub()
    }

    pub fn executions_ocr(&self) -> Result<PageSnapshot<ExecutionsSnapshot>> {
        stub()
    }

    pub fn funds_ocr(&self) -> Result<PageSnapshot<FundsSnapshot>> {
        stub()
    }

    pub fn positions_ocr(&self) -> Result<PageSnapshot<PositionsSnapshot>> {
        stub()
    }

    pub fn select_trade_security(&self, _security_code: &str) -> Result<SecuritySelectionResult> {
        stub()
    }

    pub fn stage_order(&self, _request: StageOrderRequest) -> Result<StageOrderResult> {
        stub()
    }

    pub fn trade_preflight(
        &self,
        _request: TradePreflightRequest,
    ) -> Result<TradePreflightResult> {
        stub()
    }

    pub fn open_order_confirmation(&self, _request: StageOrderRequest) -> Result<SubmitOrderResult> {
        stub()
    }

    pub fn confirm_open_order(&self, _request: StageOrderRequest) -> Result<SubmitOrderResult> {
        stub()
    }

    pub fn close_submitted_notice(&self, _contract_id: &str) -> Result<()> {
        stub()
    }

    pub fn cancel_order_confirmation(&self, _request: &StageOrderRequest) -> Result<()> {
        stub()
    }
}

fn observed_grid_table(panel: GridPanel, table: &GridTable) -> ObservedGridTable {
    let columns = table
        .columns
        .iter()
        .enumerate()
        .map(|(ordinal, title)| TableColumn {
            ordinal,
            identifier: None,
            title: ObservedText::from_value(Some(title.clone())),
            position: None,
            size: None,
        })
        .collect();
    let mut rows: Vec<TableRow> = Vec::with_capacity(table.row_count());
    rows.extend(table.rows.iter().enumerate().map(|(ordinal, cells)| TableRow {
        ordinal,
        cells: cells
            .iter()
            .map(|cell| ObservedText::from_value(Some(cell.clone())))
            .collect(),
    }));
    let quality = quality_of_rows(&rows);
    let mut warnings = vec![format!(
        "{} table was copied from the focused grid (Tab focus + Ctrl+A/Ctrl+C); no click, Enter or panel shortcut is sent",
        panel.label()
    )];
    if quality != DataQuality::Exact {
        warnings.push(
            "table structure is readable, but one or more copied cell values are empty".to_string(),
        );
    }
    ObservedGridTable {
        columns,
        rows,
        quality,
        warnings,
    }
}

fn quality_of_rows(rows: &[TableRow]) -> DataQuality {
    if rows.is_empty() {
        return DataQuality::Unavailable;
    }
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

fn stub<T>() -> Result<T> {
    Err(anyhow!("win backend not implemented (task 1 stub)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(columns: &[&str], rows: &[&[&str]]) -> GridTable {
        GridTable {
            columns: columns.iter().map(|column| column.to_string()).collect(),
            rows: rows
                .iter()
                .map(|row| row.iter().map(|cell| cell.to_string()).collect())
                .collect(),
        }
    }

    #[test]
    fn grid_table_becomes_columns_rows_and_a_clipboard_warning() {
        let observed = observed_grid_table(
            GridPanel::Positions,
            &table(
                &["证券代码", "证券名称", "当前价"],
                &[&["600018", "上港集团", "6.100"], &["600309", "万华化学", ""]],
            ),
        );
        assert_eq!(observed.columns.len(), 3);
        assert_eq!(observed.columns[0].title.value.as_deref(), Some("证券代码"));
        assert_eq!(observed.columns[0].title.quality, DataQuality::Exact);
        assert_eq!(observed.rows.len(), 2);
        assert_eq!(observed.rows[0].cells[0].value.as_deref(), Some("600018"));
        assert_eq!(observed.rows[1].cells[2].quality, DataQuality::Unavailable);
        assert_eq!(observed.quality, DataQuality::Partial);
        assert!(observed.warnings.iter().any(|warning| warning.contains("持仓")));
        assert!(observed.warnings.iter().any(|warning| warning.contains("empty")));
    }

    #[test]
    fn fully_populated_grid_rows_are_exact() {
        let observed = observed_grid_table(
            GridPanel::Executions,
            &table(&["成交编号", "成交价格"], &[&["987", "6.100"]]),
        );
        assert_eq!(observed.quality, DataQuality::Exact);
        assert_eq!(observed.warnings.len(), 1);
    }
}
