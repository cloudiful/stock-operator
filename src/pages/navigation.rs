use std::time::Duration;

use anyhow::{Result, bail};
use axuielement::AXUIElement;
use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    helpers::{
        detect_panel, detect_workspace, is_interactive, read_json_value, read_string, read_text,
    },
    reader::PageReader,
    types::{PanelKind, ViewDescriptor, WorkspaceKind},
};

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

impl PageReader {
    pub fn navigation_candidates(&self) -> Result<NavigationCandidates> {
        let Some(elements) = self.current_elements(2_000)? else {
            return Ok(NavigationCandidates {
                workspace: WorkspaceKind::Unknown,
                panel: PanelKind::Unknown,
                candidates: Vec::new(),
                warnings: vec!["target window is unavailable".to_string()],
            });
        };
        let workspace = detect_workspace(&elements);
        let panel = detect_panel(&elements);
        let candidates = navigation_candidates(&elements);
        let mut warnings = Vec::new();
        if candidates.is_empty() {
            warnings.push(
                "no allowlisted read-only navigation control was exposed by the current window"
                    .to_string(),
            );
        }
        Ok(NavigationCandidates {
            workspace,
            panel,
            candidates,
            warnings,
        })
    }

    pub fn navigate_readonly(&self, target: NavigationTarget) -> Result<NavigationResult> {
        let before = self.view()?;
        if before.panel == target.panel() {
            bail!("already on the requested read-only panel: {target:?}");
        }
        self.focus_target_window()?;
        let Some(elements) = self.current_elements(2_000)? else {
            bail!("target window is unavailable");
        };
        let candidates = navigation_candidates(&elements)
            .into_iter()
            .filter(|candidate| candidate.target == target)
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            return self.navigate_ax(
                target,
                before,
                elements,
                candidates.into_iter().next().unwrap(),
            );
        }
        if candidates.len() > 1 {
            bail!("multiple Accessibility navigation candidates were exposed for {target:?}");
        }
        super::ocr_navigation::navigate(self, target, before)
    }

    fn navigate_ax(
        &self,
        target: NavigationTarget,
        before: ViewDescriptor,
        elements: Vec<AXUIElement>,
        candidate: NavigationCandidate,
    ) -> Result<NavigationResult> {
        self.focus_target_window()?;
        let element = find_navigation_element(&elements, &candidate)?;
        element.perform_action("AXPress").map_err(|error| {
            anyhow::anyhow!("failed to press read-only navigation tab: {error:?}")
        })?;
        let mut after = self.view()?;
        for _ in 0..5 {
            if after.panel == target.panel() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            after = self.view()?;
        }
        let verified = after.panel == target.panel();
        let warnings = if verified {
            Vec::new()
        } else {
            vec![format!(
                "navigation action completed, but current panel is {:?} instead of {:?}",
                after.panel,
                target.panel()
            )]
        };
        Ok(NavigationResult {
            target,
            before,
            after,
            candidate,
            verified,
            warnings,
        })
    }
}

fn navigation_candidates(elements: &[AXUIElement]) -> Vec<NavigationCandidate> {
    let mut candidates = Vec::new();
    for element in elements.iter().filter(|element| is_interactive(element)) {
        let Some(label) = read_string(element, "AXTitle")
            .or_else(|| read_string(element, "AXDescription"))
            .or_else(|| read_text(element))
            .filter(|label| !label.trim().is_empty())
        else {
            continue;
        };
        let Some(target) = [
            NavigationTarget::Positions,
            NavigationTarget::Orders,
            NavigationTarget::Executions,
            NavigationTarget::Funds,
        ]
        .into_iter()
        .find(|target| target.labels().iter().any(|allowed| label == *allowed)) else {
            continue;
        };
        let actions = element.action_names().unwrap_or_default();
        if !actions.iter().any(|action| action == "AXPress") {
            continue;
        }
        candidates.push(NavigationCandidate {
            target,
            label,
            role: read_string(element, "AXRole"),
            subrole: read_string(element, "AXSubrole"),
            actions,
            position: read_json_value(element, "AXPosition"),
            size: read_json_value(element, "AXSize"),
        });
    }
    candidates
}

fn find_navigation_element(
    elements: &[AXUIElement],
    candidate: &NavigationCandidate,
) -> Result<AXUIElement> {
    let matches = elements
        .iter()
        .filter(|element| {
            is_interactive(element)
                && read_string(element, "AXRole") == candidate.role
                && read_string(element, "AXSubrole") == candidate.subrole
                && read_string(element, "AXTitle")
                    .or_else(|| read_string(element, "AXDescription"))
                    .or_else(|| read_text(element))
                    .is_some_and(|label| label == candidate.label)
                && element
                    .action_names()
                    .unwrap_or_default()
                    .iter()
                    .any(|action| action == "AXPress")
        })
        .cloned()
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!(
            "navigation candidate became ambiguous before action: found {} matching controls",
            matches.len()
        );
    }
    Ok(matches.into_iter().next().expect("match count checked"))
}
