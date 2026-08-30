use anyhow::Result;

use super::{
    helpers::{
        detect_panel, detect_workspace, table_columns, table_matches_header_count, table_quality,
        table_rows,
    },
    reader::PageReader,
    types::{
        ExecutionsSnapshot, FundsSnapshot, OrdersSnapshot, PageSnapshot, PanelKind, WorkspaceKind,
        empty_snapshot, snapshot,
    },
};

impl PageReader {
    pub fn cancellations(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        let (workspace, panel, table, mut warnings) = self.find_panel_table(
            PanelKind::Cancellations,
            &["委托编号", "买卖标志", "委托状态", "撤单标志", "证券代码"],
        )?;
        let rows = table
            .as_ref()
            .map(|table| table_rows(table, 500))
            .unwrap_or_default();
        let columns = table
            .as_ref()
            .map(|table| table_columns(table))
            .unwrap_or_default();
        if table.is_some() && rows.is_empty() {
            warnings.push("cancellations table exposed no rows".to_string());
        }
        let quality = table_quality(&rows);
        Ok(snapshot(
            workspace,
            panel,
            OrdersSnapshot {
                row_count: rows.len(),
                columns,
                rows,
            },
            quality,
            warnings,
        ))
    }

    pub fn orders(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        let (workspace, panel, table, mut warnings) = self.find_panel_table(
            PanelKind::Orders,
            &["委托状态", "委托编号", "委托价格", "委托数量", "证券代码"],
        )?;
        let rows = table
            .as_ref()
            .map(|table| table_rows(table, 500))
            .unwrap_or_default();
        let columns = table
            .as_ref()
            .map(|table| table_columns(table))
            .unwrap_or_default();
        let row_count = rows.len();
        if table.is_some() && row_count == 0 {
            warnings.push("orders table exposed no rows".to_string());
        }
        let quality = table_quality(&rows);
        Ok(snapshot(
            workspace,
            panel,
            OrdersSnapshot {
                columns,
                rows,
                row_count,
            },
            quality,
            warnings,
        ))
    }

    pub fn executions(&self) -> Result<PageSnapshot<ExecutionsSnapshot>> {
        let (workspace, panel, table, mut warnings) = self.find_panel_table(
            PanelKind::Executions,
            &["成交编号", "成交价格", "成交数量", "成交金额", "证券代码"],
        )?;
        let rows = table
            .as_ref()
            .map(|table| table_rows(table, 500))
            .unwrap_or_default();
        let columns = table
            .as_ref()
            .map(|table| table_columns(table))
            .unwrap_or_default();
        let row_count = rows.len();
        if table.is_some() && row_count == 0 {
            warnings.push("executions table exposed no rows".to_string());
        }
        let quality = table_quality(&rows);
        Ok(snapshot(
            workspace,
            panel,
            ExecutionsSnapshot {
                columns,
                rows,
                row_count,
            },
            quality,
            warnings,
        ))
    }

    pub fn funds(&self) -> Result<PageSnapshot<FundsSnapshot>> {
        let Some(elements) = self.current_elements(2_000)? else {
            return Ok(empty_snapshot(
                WorkspaceKind::Unknown,
                PanelKind::Funds,
                FundsSnapshot {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    row_count: 0,
                },
                vec!["target window is unavailable".to_string()],
            ));
        };
        let workspace = detect_workspace(&elements);
        let table = elements.iter().find(|element| {
            table_matches_header_count(element, &["可用资金", "冻结资金", "资金余额", "总资产"], 2)
        });
        let Some(table) = table else {
            return Ok(empty_snapshot(
                workspace,
                PanelKind::Funds,
                FundsSnapshot {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    row_count: 0,
                },
                vec!["funds table was not exposed by the current window".to_string()],
            ));
        };
        let rows = table_rows(table, 200);
        let row_count = rows.len();
        let mut warnings = Vec::new();
        if row_count == 0 {
            warnings.push("funds table exposed no rows".to_string());
        }
        let quality = table_quality(&rows);
        Ok(snapshot(
            workspace,
            PanelKind::Funds,
            FundsSnapshot {
                columns: table_columns(table),
                rows,
                row_count,
            },
            quality,
            warnings,
        ))
    }

    fn find_panel_table(
        &self,
        panel: PanelKind,
        keywords: &[&str],
    ) -> Result<(
        WorkspaceKind,
        PanelKind,
        Option<axuielement::AXUIElement>,
        Vec<String>,
    )> {
        let Some(elements) = self.current_elements(2_000)? else {
            return Ok((
                WorkspaceKind::Unknown,
                panel,
                None,
                vec!["target window is unavailable".to_string()],
            ));
        };
        let workspace = detect_workspace(&elements);
        let detected_panel = detect_panel(&elements);
        let table = elements
            .iter()
            .find(|element| table_matches_header_count(element, keywords, 2))
            .cloned();
        let warnings = if detected_panel != PanelKind::Unknown && detected_panel != panel {
            vec![format!(
                "current panel is {detected_panel:?}, not the requested {panel:?}"
            )]
        } else if table.is_none() {
            vec![format!(
                "{panel:?} table was not exposed by the current window"
            )]
        } else {
            Vec::new()
        };
        let table = if detected_panel != PanelKind::Unknown && detected_panel != panel {
            None
        } else {
            table
        };
        Ok((workspace, panel, table, warnings))
    }
}
