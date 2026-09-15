//! 持仓/委托/成交 grid reads for the 恒生/至胜 terminal (`xiadan.exe`).
//!
//! The three panels are owner-drawn `CVirtualGridCtrl` tables: their rows are not
//! exposed through `GetWindowTextW`, so the table is copied out of the terminal
//! and parsed as clipboard TSV.
//!
//! Safety rules of this read path (phase Task 4):
//!
//! - the only synthesized keys are `Tab` (grid focus), `Ctrl+A` and `Ctrl+C`
//!   (copy); never a click, `Enter`, an order/cancel control or a panel shortcut;
//! - reads are read-only: the panel that is displayed is the panel that is read,
//!   so a mismatch fails loudly instead of returning another table's rows;
//! - an empty copy is an error, never an empty table.
//!
//! The 和讯 panes track keyboard focus inside the pane window (`GUITHREADINFO`
//! reports no focused child while the terminal is active), so the focus is not
//! introspected: the copied payload itself decides whether the grid was reached,
//! and `Tab` is only pressed to try the next control after a failed copy.

mod clipboard;
mod keys;

use anyhow::{Result, bail};

/// Columns that identify a panel in a copied grid.
const POSITION_MARKERS: [&str; 6] =
    ["参考持股", "可用股份", "成本价", "浮动盈亏", "股份余额", "最新市值"];
const ORDER_MARKERS: [&str; 6] =
    ["委托编号", "委托时间", "委托价格", "委托数量", "委托状态", "委托类型"];
const EXECUTION_MARKERS: [&str; 5] = ["成交编号", "成交时间", "成交价格", "成交金额", "成交日期"];

/// Copy attempts before giving up; every attempt after the first presses `Tab`
/// once, which is the task's focus budget of at most 20 presses.
const MAX_COPY_ATTEMPTS: usize = 20;

/// One of the three panels that are read through the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridPanel {
    Positions,
    Orders,
    Executions,
}

impl GridPanel {
    pub const ALL: [GridPanel; 3] = [Self::Positions, Self::Orders, Self::Executions];

    pub fn label(self) -> &'static str {
        match self {
            Self::Positions => "持仓",
            Self::Orders => "委托",
            Self::Executions => "成交",
        }
    }

    /// Columns that separate this panel from the other two.
    fn header_markers(self) -> &'static [&'static str] {
        match self {
            Self::Positions => &POSITION_MARKERS,
            Self::Orders => &ORDER_MARKERS,
            Self::Executions => &EXECUTION_MARKERS,
        }
    }
}

/// A grid copied from the terminal: the header row plus the data rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl GridTable {
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

/// Reads `panel` from the grid the terminal is currently displaying.
///
/// Fails when the terminal is missing or minimized, when no grid can be copied
/// from the focused control, when the copy is not a usable table, or when the
/// displayed panel is not `panel`.
pub fn read_grid(main_hwnd: isize, panel: GridPanel) -> Result<GridTable> {
    if main_hwnd == 0 {
        bail!("a terminal main window handle is required to read the {} grid", panel.label());
    }
    keys::ensure_readable(main_hwnd)?;
    let mut last_error = String::from("the copy was never attempted");
    for attempt in 0..MAX_COPY_ATTEMPTS {
        if attempt > 0 {
            keys::press_tab(main_hwnd)?;
        }
        match copy_grid(main_hwnd).and_then(|text| parse_tsv(&text)) {
            Ok(table) => {
                verify_panel(&table, panel)?;
                return Ok(table);
            }
            Err(error) => last_error = format!("{error:#}"),
        }
    }
    bail!(
        "no {} grid (class {}) could be copied after {MAX_COPY_ATTEMPTS} attempts (last error: {last_error}); open the {} page, click once inside the table and retry — only Tab/Ctrl+A/Ctrl+C are ever sent",
        panel.label(),
        keys::GRID_CLASS_PREFIX,
        panel.label()
    )
}

/// `Ctrl+A` then `Ctrl+C` on the focused control, then the clipboard payload.
///
/// The clipboard is emptied first so a previous copy can never be mistaken for
/// this one's result.
fn copy_grid(main_hwnd: isize) -> Result<String> {
    clipboard::clear()?;
    keys::press_ctrl(main_hwnd, keys::VK_A)?;
    keys::press_ctrl(main_hwnd, keys::VK_C)?;
    clipboard::wait_for_text()
}

/// Parses one copied grid.
///
/// The first non-empty line is the header row and every following line is a data
/// row: rows shorter than the header are padded, extra cells are dropped, and
/// blank lines are ignored. A copy without a header row or without data rows is
/// an error so a failed copy can never look like an empty table.
pub fn parse_tsv(text: &str) -> Result<GridTable> {
    let mut lines = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r').trim_end());
    let Some(header) = lines.by_ref().find(|line| !line.is_empty()) else {
        bail!("the copied grid was empty: it held no header row");
    };
    let columns: Vec<String> = header.split('\t').map(|cell| cell.trim().to_string()).collect();
    if columns.iter().all(|column| column.is_empty()) {
        bail!("the copied grid header row held no column names");
    }
    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let mut cells: Vec<String> = line.split('\t').map(|cell| cell.trim().to_string()).collect();
        if cells.iter().all(|cell| cell.is_empty()) {
            continue;
        }
        cells.truncate(columns.len());
        cells.resize(columns.len(), String::new());
        rows.push(cells);
    }
    if rows.is_empty() {
        bail!("the copied grid had a header row but no data rows");
    }
    Ok(GridTable { columns, rows })
}

/// Identifies the panel from the copied header row; a tie or no match stays
/// unknown instead of guessing.
pub fn detect_panel(table: &GridTable) -> Option<GridPanel> {
    let headers: Vec<String> = table.columns.iter().map(|column| normalize_header(column)).collect();
    let scored: Vec<(usize, GridPanel)> = GridPanel::ALL
        .into_iter()
        .map(|panel| (panel_score(panel, &headers), panel))
        .filter(|(score, _)| *score > 0)
        .collect();
    let best = scored.iter().map(|(score, _)| *score).max()?;
    let winners: Vec<GridPanel> = scored
        .iter()
        .filter(|(score, _)| *score == best)
        .map(|(_, panel)| *panel)
        .collect();
    (winners.len() == 1).then(|| winners[0])
}

/// Fails unless the copied grid belongs to `panel`.
pub fn verify_panel(table: &GridTable, panel: GridPanel) -> Result<()> {
    match detect_panel(table) {
        Some(detected) if detected == panel => Ok(()),
        Some(detected) => bail!(
            "the displayed grid is {} but {} was requested: open the {} page with its bottom shortcut and retry (panels are never switched automatically)",
            detected.label(),
            panel.label(),
            panel.label()
        ),
        None => bail!(
            "the copied header row was not recognized as 持仓/委托/成交: {:?}",
            table.columns
        ),
    }
}

fn panel_score(panel: GridPanel, headers: &[String]) -> usize {
    panel
        .header_markers()
        .iter()
        .filter(|marker| headers.iter().any(|header| header.contains(**marker)))
        .count()
}

fn normalize_header(column: &str) -> String {
    column.chars().filter(|character| !character.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSITIONS_TSV: &str = "证券代码\t证券名称\t参考持股\t可用股份\t成本价\t当前价\t浮动盈亏\r\n\
600018\t上港集团\t1000\t1000\t5.900\t6.100\t200.000\r\n\
600309\t万华化学\t500\t500\t80.000\t81.500\t750.000\t\r\n\
\r\n\
000807\t云铝股份\t200\t200\r\n";

    const ORDERS_TSV: &str = "委托编号\t委托时间\t证券代码\t证券名称\t买卖方向\t委托价格\t委托数量\t委托状态\n\
12345\t09:31:02\t600018\t上港集团\t买入\t6.100\t100\t已报\n";

    const EXECUTIONS_TSV: &str = "成交编号\t成交时间\t证券代码\t证券名称\t买卖方向\t成交价格\t成交数量\t成交金额\n\
987\t09:31:05\t600018\t上港集团\t买入\t6.100\t100\t610.00\n";

    #[test]
    fn parses_a_copied_positions_grid() {
        let table = parse_tsv(POSITIONS_TSV).expect("parsable");
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.columns[0], "证券代码");
        assert_eq!(table.columns[6], "浮动盈亏");
        assert_eq!(table.rows[0][0], "600018");
        assert_eq!(table.rows[0][1], "上港集团");
        assert_eq!(table.rows[1].len(), 7, "the trailing tab must not create a column");
        assert_eq!(table.rows[2], ["000807", "云铝股份", "200", "200", "", "", ""]);
    }

    #[test]
    fn detects_each_panel_and_rejects_a_mismatch() {
        let positions = parse_tsv(POSITIONS_TSV).expect("parsable");
        let orders = parse_tsv(ORDERS_TSV).expect("parsable");
        let executions = parse_tsv(EXECUTIONS_TSV).expect("parsable");
        assert_eq!(detect_panel(&positions), Some(GridPanel::Positions));
        assert_eq!(detect_panel(&orders), Some(GridPanel::Orders));
        assert_eq!(detect_panel(&executions), Some(GridPanel::Executions));
        assert!(verify_panel(&positions, GridPanel::Positions).is_ok());
        let mismatch = verify_panel(&positions, GridPanel::Orders).expect_err("mismatch");
        assert!(mismatch.to_string().contains("持仓"), "{mismatch}");
        assert!(mismatch.to_string().contains("委托"), "{mismatch}");
    }

    #[test]
    fn unknown_or_ambiguous_headers_stay_unrecognized() {
        let unknown = GridTable {
            columns: vec!["编号".to_string(), "名称".to_string()],
            rows: vec![vec!["1".to_string(), "2".to_string()]],
        };
        assert_eq!(detect_panel(&unknown), None);
        assert!(verify_panel(&unknown, GridPanel::Positions).is_err());
        let ambiguous = GridTable {
            columns: vec!["委托编号".to_string(), "成交编号".to_string()],
            rows: vec![vec!["1".to_string(), "2".to_string()]],
        };
        assert_eq!(detect_panel(&ambiguous), None);
    }

    #[test]
    fn rejects_an_empty_copy_instead_of_returning_an_empty_table() {
        assert!(parse_tsv("").is_err());
        assert!(parse_tsv("\r\n \r\n").is_err());
        let header_only = parse_tsv("证券代码\t证券名称\t参考持股\t可用股份\n");
        assert!(header_only.is_err(), "{header_only:?}");
        let unlabeled = parse_tsv("\t\t\n\t\t\n");
        assert!(unlabeled.is_err());
    }

    #[test]
    fn read_grid_requires_a_terminal_handle() {
        let error = read_grid(0, GridPanel::Positions).expect_err("no handle");
        assert!(error.to_string().contains("持仓"), "{error}");
    }

    #[test]
    fn panel_labels_and_shortcut_identity() {
        assert_eq!(GridPanel::ALL.map(GridPanel::label), ["持仓", "委托", "成交"]);
        assert_eq!(keys::GRID_CLASS_PREFIX, "CVirtualGridCtrl");
    }
}
