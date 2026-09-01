use anyhow::Result;

use super::{
    helpers::{collect_elements, detect_workspace, is_positions_table},
    ocr_table::read_table,
    reader::PageReader,
    types::{
        DataQuality, DataSource, PageSnapshot, PanelKind, PositionsSnapshot, WorkspaceKind,
        snapshot_with_source,
    },
};

impl PageReader {
    pub fn positions_ocr(&self) -> Result<PageSnapshot<PositionsSnapshot>> {
        let ocr = self.ocr_visible_text()?;
        let Some(window) = self.current_window()? else {
            return Ok(snapshot_with_source(
                WorkspaceKind::Unknown,
                PanelKind::Positions,
                PositionsSnapshot {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    row_count: 0,
                },
                DataQuality::Unavailable,
                DataSource::Ocr,
                vec!["target window is unavailable".to_string()],
            ));
        };
        let elements = collect_elements(&window, 2_000);
        let workspace = detect_workspace(&elements);
        let Some(table) = elements.iter().find(|element| is_positions_table(element)) else {
            return Ok(snapshot_with_source(
                workspace,
                PanelKind::Positions,
                PositionsSnapshot {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    row_count: 0,
                },
                DataQuality::Unavailable,
                DataSource::Ocr,
                vec!["positions table was not exposed by the current window".to_string()],
            ));
        };
        let result = read_table(table, &ocr, &window, PanelKind::Positions)?;
        Ok(snapshot_with_source(
            workspace,
            PanelKind::Positions,
            PositionsSnapshot {
                row_count: result.rows.len(),
                columns: result.columns,
                rows: result.rows,
            },
            result.quality,
            DataSource::Ocr,
            result.warnings,
        ))
    }
}
