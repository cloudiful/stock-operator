use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{PanelKind, ViewDescriptor, WorkspaceKind};

#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Eq,
    JsonSchema,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum NavigationTarget {
    Positions,
    Orders,
    Executions,
    Funds,
}

impl NavigationTarget {
    pub(crate) const ALL: [Self; 4] =
        [Self::Positions, Self::Orders, Self::Executions, Self::Funds];

    pub(crate) fn panel(self) -> PanelKind {
        match self {
            Self::Positions => PanelKind::Positions,
            Self::Orders => PanelKind::Orders,
            Self::Executions => PanelKind::Executions,
            Self::Funds => PanelKind::Funds,
        }
    }

    pub(crate) fn labels(self) -> &'static [&'static str] {
        match self {
            Self::Positions => &["持仓"],
            Self::Orders => &["当日委托", "委托"],
            Self::Executions => &["当日成交", "成交"],
            Self::Funds => &["资金明细"],
        }
    }
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct NavigationCandidate {
    pub target: NavigationTarget,
    pub label: String,
    pub role: Option<String>,
    pub subrole: Option<String>,
    pub actions: Vec<String>,
    pub position: Option<serde_json::Value>,
    pub size: Option<serde_json::Value>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct NavigationCandidates {
    pub workspace: WorkspaceKind,
    pub panel: PanelKind,
    pub candidates: Vec<NavigationCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct NavigationResult {
    pub target: NavigationTarget,
    pub before: ViewDescriptor,
    pub after: ViewDescriptor,
    pub candidate: NavigationCandidate,
    pub verified: bool,
    pub warnings: Vec<String>,
}
