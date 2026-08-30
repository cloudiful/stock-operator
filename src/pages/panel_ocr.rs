use anyhow::Result;
use schemars::JsonSchema;

use super::{
    helpers::{collect_elements, detect_panel, detect_workspace},
    ocr_table::{find_table, read_table},
    reader::PageReader,
    types::{
        DataSource, ExecutionsSnapshot, FundsSnapshot, OrdersSnapshot, PageSnapshot, PanelKind,
        WorkspaceKind, empty_snapshot, snapshot_with_source,
    },
};

impl PageReader {
    pub fn cancellations_ocr(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        read_panel_ocr(
            self,
            PanelKind::Cancellations,
            &["委托编号", "买卖标志", "委托状态", "撤单标志", "证券代码"],
            3,
            "cancellations",
            |result| OrdersSnapshot {
                row_count: result.rows.len(),
                columns: result.columns,
                rows: result.rows,
            },
        )
    }

    pub fn orders_ocr(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        read_panel_ocr(
            self,
            PanelKind::Orders,
            &["委托编号", "委托状态", "委托价格", "委托数量", "证券代码"],
            3,
            "orders",
            |result| OrdersSnapshot {
                row_count: result.rows.len(),
                columns: result.columns,
                rows: result.rows,
            },
        )
    }

    pub fn executions_ocr(&self) -> Result<PageSnapshot<ExecutionsSnapshot>> {
        read_panel_ocr(
            self,
            PanelKind::Executions,
            &["成交编号", "成交价格", "成交数量", "成交金额", "证券代码"],
            3,
            "executions",
            |result| ExecutionsSnapshot {
                row_count: result.rows.len(),
                columns: result.columns,
                rows: result.rows,
            },
        )
    }

    pub fn funds_ocr(&self) -> Result<PageSnapshot<FundsSnapshot>> {
        read_panel_ocr(
            self,
            PanelKind::Funds,
            &["可用资金", "冻结资金", "资金余额", "总资产"],
            2,
            "funds",
            |result| FundsSnapshot {
                row_count: result.rows.len(),
                columns: result.columns,
                rows: result.rows,
            },
        )
    }
}

fn read_panel_ocr<T, Build>(
    reader: &PageReader,
    panel: PanelKind,
    headers: &[&str],
    minimum_matches: usize,
    panel_name: &str,
    build: Build,
) -> Result<PageSnapshot<T>>
where
    T: JsonSchema,
    Build: FnOnce(super::ocr_table::OcrTableResult) -> T,
{
    let ocr = reader.ocr_visible_text()?;
    let Some(window) = reader.current_window()? else {
        return Ok(empty_snapshot(
            WorkspaceKind::Unknown,
            panel,
            build(super::ocr_table::OcrTableResult {
                columns: Vec::new(),
                rows: Vec::new(),
                quality: super::types::DataQuality::Unavailable,
                warnings: Vec::new(),
            }),
            vec!["target window is unavailable".to_string()],
        ));
    };
    let elements = collect_elements(&window, 2_000);
    let workspace = detect_workspace(&elements);
    let detected_panel = detect_panel(&elements);
    if detected_panel != PanelKind::Unknown && detected_panel != panel {
        return Ok(empty_snapshot(
            workspace,
            panel,
            build(super::ocr_table::OcrTableResult {
                columns: Vec::new(),
                rows: Vec::new(),
                quality: super::types::DataQuality::Unavailable,
                warnings: Vec::new(),
            }),
            vec![format!(
                "current panel is {detected_panel:?}, not the requested {panel:?}"
            )],
        ));
    }
    let Some(table) = find_table(&elements, headers, minimum_matches) else {
        return Ok(empty_snapshot(
            workspace,
            panel,
            build(super::ocr_table::OcrTableResult {
                columns: Vec::new(),
                rows: Vec::new(),
                quality: super::types::DataQuality::Unavailable,
                warnings: Vec::new(),
            }),
            vec![format!(
                "{panel_name} table signature was not exposed by the current window"
            )],
        ));
    };
    let result = read_table(table, &ocr, &window, panel)?;
    let quality = result.quality;
    let warnings = result.warnings.clone();
    Ok(snapshot_with_source(
        workspace,
        panel,
        build(result),
        quality,
        DataSource::Ocr,
        warnings,
    ))
}
