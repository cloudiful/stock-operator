use schemars::JsonSchema;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    Unknown,
    BuyOrder,
    SellOrder,
}

#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    Unknown,
    Positions,
    Orders,
    Executions,
    Cancellations,
    Funds,
}

#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataQuality {
    Exact,
    Partial,
    Unavailable,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    Accessibility,
    Ocr,
    Inferred,
    Unavailable,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct ObservedText {
    pub value: Option<String>,
    pub quality: DataQuality,
    pub source: DataSource,
}

impl ObservedText {
    pub(crate) fn unavailable() -> Self {
        Self {
            value: None,
            quality: DataQuality::Unavailable,
            source: DataSource::Unavailable,
        }
    }

    pub(crate) fn from_value(value: Option<String>) -> Self {
        match value {
            Some(value) if !value.trim().is_empty() => Self {
                value: Some(value),
                quality: DataQuality::Exact,
                source: DataSource::Accessibility,
            },
            _ => Self::unavailable(),
        }
    }

    pub(crate) fn from_ocr(value: String, confidence: f32) -> Self {
        let redacted = value_is_redacted(&value);
        Self {
            value: Some(value),
            quality: if redacted {
                DataQuality::Partial
            } else if confidence >= 0.9 {
                DataQuality::Exact
            } else {
                DataQuality::Partial
            },
            source: DataSource::Ocr,
        }
    }

    pub(crate) fn redacted() -> Self {
        Self {
            value: Some("[REDACTED]".to_string()),
            quality: DataQuality::Partial,
            source: DataSource::Ocr,
        }
    }
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct PageSnapshot<T: JsonSchema> {
    pub workspace: WorkspaceKind,
    pub panel: PanelKind,
    pub observed_at: String,
    pub quality: DataQuality,
    pub source: DataSource,
    pub warnings: Vec<String>,
    pub data: T,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct ControlDescriptor {
    pub role: Option<String>,
    pub subrole: Option<String>,
    pub title: Option<String>,
    pub identifier: Option<String>,
    pub description: Option<String>,
    pub value: ObservedText,
    pub actions: Vec<String>,
    pub position: Option<serde_json::Value>,
    pub size: Option<serde_json::Value>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct UiInventory {
    pub workspace: WorkspaceKind,
    pub panel: PanelKind,
    pub controls: Vec<ControlDescriptor>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct ViewDescriptor {
    pub workspace: WorkspaceKind,
    pub panel: PanelKind,
    pub evidence: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct TradeFormSnapshot {
    pub controls: Vec<TradeFormControl>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct TradeFormControl {
    pub ordinal: usize,
    pub role: String,
    pub value: ObservedText,
    pub focused: Option<bool>,
    pub position: Option<serde_json::Value>,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct PositionsSnapshot {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    pub row_count: usize,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct OrdersSnapshot {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    pub row_count: usize,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct ExecutionsSnapshot {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    pub row_count: usize,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct FundsSnapshot {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
    pub row_count: usize,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct PositionsTableDiagnostic {
    pub table: Option<TableDiagnosticElement>,
    pub rows: Vec<TableDiagnosticElement>,
    pub exposed_row_count: usize,
    pub inspected_row_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct TableDiagnosticElement {
    pub role: Option<String>,
    pub subrole: Option<String>,
    pub title: Option<String>,
    pub identifier: Option<String>,
    pub description: Option<String>,
    pub values: std::collections::BTreeMap<String, serde_json::Value>,
    pub attributes: Vec<String>,
    pub actions: Vec<String>,
    pub child_count: usize,
    pub children: Vec<TableDiagnosticElement>,
    pub redacted: bool,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct TableColumn {
    pub ordinal: usize,
    pub identifier: Option<String>,
    pub title: ObservedText,
    pub position: Option<serde_json::Value>,
    pub size: Option<serde_json::Value>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct TableRow {
    pub ordinal: usize,
    pub cells: Vec<ObservedText>,
}

pub(crate) fn snapshot<T: JsonSchema>(
    workspace: WorkspaceKind,
    panel: PanelKind,
    data: T,
    quality: DataQuality,
    warnings: Vec<String>,
) -> PageSnapshot<T> {
    PageSnapshot {
        workspace,
        panel,
        observed_at: now_string(),
        quality,
        source: DataSource::Accessibility,
        warnings,
        data,
    }
}

pub(crate) fn empty_snapshot<T: JsonSchema>(
    workspace: WorkspaceKind,
    panel: PanelKind,
    data: T,
    warnings: Vec<String>,
) -> PageSnapshot<T> {
    PageSnapshot {
        workspace,
        panel,
        observed_at: now_string(),
        quality: DataQuality::Unavailable,
        source: DataSource::Unavailable,
        warnings,
        data,
    }
}

pub(crate) fn snapshot_with_source<T: JsonSchema>(
    workspace: WorkspaceKind,
    panel: PanelKind,
    data: T,
    quality: DataQuality,
    source: DataSource,
    warnings: Vec<String>,
) -> PageSnapshot<T> {
    PageSnapshot {
        workspace,
        panel,
        observed_at: now_string(),
        quality,
        source,
        warnings,
        data,
    }
}

fn value_is_redacted(value: &str) -> bool {
    value == "[REDACTED]" || value == "[REDACTED_NUMERIC]"
}

fn now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
