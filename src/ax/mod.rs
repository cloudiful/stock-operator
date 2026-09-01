use std::{collections::BTreeMap, process::Command, thread, time::Duration};

use anyhow::{Context, Result, bail};
use axuielement::{AXUIElement, AXValue, AXValueKind, process_trust};
use schemars::JsonSchema;
use serde::Serialize;

use super::config::OperatorConfig;

const AX_ROLE: &str = "AXRole";
const AX_SUBROLE: &str = "AXSubrole";
const AX_TITLE: &str = "AXTitle";
const AX_DESCRIPTION: &str = "AXDescription";
const AX_IDENTIFIER: &str = "AXIdentifier";
const AX_VALUE: &str = "AXValue";
const AX_HELP: &str = "AXHelp";
const AX_ENABLED: &str = "AXEnabled";
const AX_FOCUSED: &str = "AXFocused";
const AX_POSITION: &str = "AXPosition";
const AX_SIZE: &str = "AXSize";

#[derive(Clone)]
pub struct AccessibilityInspector {
    config: OperatorConfig,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AccessibilityStatus {
    pub api_enabled: bool,
    pub process_trusted: bool,
    pub target_process_name: String,
    pub target_bundle_id: String,
    pub target_pid: Option<i32>,
    pub target_found: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TargetSnapshot {
    pub status: AccessibilityStatus,
    pub root: Option<ElementSnapshot>,
    pub node_count: usize,
    pub truncated: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ElementSnapshot {
    pub role: Option<String>,
    pub subrole: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub identifier: Option<String>,
    pub help: Option<String>,
    pub value: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub focused: Option<bool>,
    pub position: Option<serde_json::Value>,
    pub size: Option<serde_json::Value>,
    pub actions: Vec<String>,
    pub attributes: Vec<String>,
    pub children: Vec<ElementSnapshot>,
}

impl AccessibilityInspector {
    pub fn new(config: OperatorConfig) -> Self {
        Self { config }
    }

    pub fn status(&self) -> AccessibilityStatus {
        let target_pid = find_target_pid(
            &self.config.target_process_name,
            &self.config.target_bundle_id,
        );
        let mut notes = Vec::new();
        if !process_trust::api_enabled() {
            notes.push("macOS Accessibility API is disabled".to_string());
        }
        if !process_trust::is_process_trusted() {
            notes.push(
                "grant Accessibility permission to the stock-operator process in System Settings"
                    .to_string(),
            );
        }
        if target_pid.is_none() {
            notes.push(
                "target process was not found with the configured process name and bundle ID"
                    .to_string(),
            );
        }

        AccessibilityStatus {
            api_enabled: process_trust::api_enabled(),
            process_trusted: process_trust::is_process_trusted(),
            target_process_name: self.config.target_process_name.clone(),
            target_bundle_id: self.config.target_bundle_id.clone(),
            target_pid,
            target_found: target_pid.is_some(),
            notes,
        }
    }

    pub fn target_window(&self) -> Result<Option<AXUIElement>> {
        let status = self.status();
        let Some(pid) = status.target_pid else {
            return Ok(None);
        };
        if !status.process_trusted {
            return Ok(None);
        }

        let application = AXUIElement::from_pid(pid)
            .context("AXUIElementCreateApplication returned no element")?;
        application
            .set_timeout(0.75)
            .map_err(|error| anyhow::anyhow!("failed to set AX timeout: {error:?}"))?;
        Ok(Some(preferred_window(&application).unwrap_or(application)))
    }

    pub fn focus_target_window(&self) -> Result<AXUIElement> {
        let status = self.status();
        let Some(pid) = status.target_pid else {
            bail!("target process is not running");
        };
        if !status.process_trusted {
            bail!("Accessibility permission is not granted to stock-operator");
        }
        let application = AXUIElement::from_pid(pid)
            .context("AXUIElementCreateApplication returned no element")?;
        application
            .set_timeout(0.75)
            .map_err(|error| anyhow::anyhow!("failed to set AX timeout: {error:?}"))?;
        let Some(window) = preferred_window(&application) else {
            bail!("target application has no accessible window");
        };
        if window.pid().map_err(|error| {
            anyhow::anyhow!("failed to identify target window process: {error:?}")
        })? != pid
        {
            bail!("target window does not belong to the configured application");
        }
        if target_is_focused(&application, pid) {
            return Ok(window);
        }
        application
            .set_bool_attribute("AXFrontmost", true)
            .map_err(|error| {
                anyhow::anyhow!("failed to make target application frontmost: {error:?}")
            })?;
        window.perform_action("AXRaise").map_err(|error| {
            anyhow::anyhow!("failed to raise target application window: {error:?}")
        })?;
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(50));
            if target_is_focused(&application, pid) {
                let focused = preferred_window(&application)
                    .context("target window disappeared after it was focused")?;
                if focused.pid().ok() == Some(pid) {
                    return Ok(focused);
                }
                bail!("focused window no longer belongs to the configured application");
            }
        }
        bail!("target application window could not be focused")
    }

    pub fn snapshot(&self, max_depth: usize, max_nodes: usize) -> Result<TargetSnapshot> {
        let status = self.status();
        let Some(pid) = status.target_pid else {
            return Ok(TargetSnapshot {
                status,
                root: None,
                node_count: 0,
                truncated: false,
            });
        };
        if !status.process_trusted {
            return Ok(TargetSnapshot {
                status,
                root: None,
                node_count: 0,
                truncated: false,
            });
        }

        let application = AXUIElement::from_pid(pid)
            .context("AXUIElementCreateApplication returned no element")?;
        application
            .set_timeout(0.75)
            .map_err(|error| anyhow::anyhow!("failed to set AX timeout: {error:?}"))?;
        let root = preferred_window(&application).unwrap_or(application);

        let mut state = WalkState {
            max_depth,
            max_nodes,
            node_count: 0,
            truncated: false,
        };
        let root = walk_element(&root, 0, &mut state)?;
        Ok(TargetSnapshot {
            status,
            root: Some(root),
            node_count: state.node_count,
            truncated: state.truncated,
        })
    }
}

fn target_is_focused(application: &AXUIElement, pid: i32) -> bool {
    let application_is_frontmost =
        application.bool_attribute("AXFrontmost").ok().flatten() == Some(true);
    let window_matches = application
        .element_attribute("AXFocusedWindow")
        .ok()
        .flatten()
        .and_then(|window| window.pid().ok())
        == Some(pid);
    application_is_frontmost && window_matches
}

fn preferred_window(application: &AXUIElement) -> Option<AXUIElement> {
    application
        .element_attribute("AXFocusedWindow")
        .ok()
        .flatten()
        .or_else(|| application.element_attribute("AXMainWindow").ok().flatten())
        .or_else(|| {
            application
                .element_array_attribute("AXWindows")
                .ok()
                .and_then(|windows| windows.into_iter().next())
        })
}

struct WalkState {
    max_depth: usize,
    max_nodes: usize,
    node_count: usize,
    truncated: bool,
}

fn walk_element(
    element: &AXUIElement,
    depth: usize,
    state: &mut WalkState,
) -> Result<ElementSnapshot> {
    if state.node_count >= state.max_nodes {
        state.truncated = true;
        anyhow::bail!("Accessibility tree walk reached its node limit")
    }
    state.node_count += 1;

    let attributes = element
        .attribute_names()
        .unwrap_or_default()
        .into_iter()
        .take(64)
        .collect::<Vec<_>>();
    let role = read_string(element, AX_ROLE);
    let secure = role.as_deref() == Some("AXSecureTextField");
    let children = if depth < state.max_depth {
        let source_children = element.children().unwrap_or_default();
        let total_children = source_children.len();
        let mut children = Vec::new();
        for child in source_children {
            if state.node_count >= state.max_nodes {
                if children.len() < total_children {
                    state.truncated = true;
                }
                break;
            }
            children.push(walk_element(&child, depth + 1, state)?);
        }
        children
    } else {
        if element
            .children()
            .map(|items| !items.is_empty())
            .unwrap_or(false)
        {
            state.truncated = true;
        }
        Vec::new()
    };

    Ok(ElementSnapshot {
        role,
        subrole: read_string(element, AX_SUBROLE),
        title: read_string(element, AX_TITLE),
        description: read_string(element, AX_DESCRIPTION),
        identifier: read_string(element, AX_IDENTIFIER),
        help: read_string(element, AX_HELP),
        value: if secure {
            None
        } else {
            read_value(element, AX_VALUE)
        },
        enabled: read_bool(element, AX_ENABLED),
        focused: read_bool(element, AX_FOCUSED),
        position: read_value(element, AX_POSITION),
        size: read_value(element, AX_SIZE),
        actions: element.action_names().unwrap_or_default(),
        attributes,
        children,
    })
}

fn read_string(element: &AXUIElement, attribute: &str) -> Option<String> {
    element.string_attribute(attribute).ok().flatten()
}

fn read_bool(element: &AXUIElement, attribute: &str) -> Option<bool> {
    element.bool_attribute(attribute).ok().flatten()
}

fn read_value(element: &AXUIElement, attribute: &str) -> Option<serde_json::Value> {
    element
        .attribute(attribute)
        .ok()
        .flatten()
        .and_then(|value| value_to_json(&value, 0))
}

pub(crate) fn attribute_value_json(
    element: &AXUIElement,
    attribute: &str,
) -> Option<serde_json::Value> {
    read_value(element, attribute)
}

fn value_to_json(value: &AXValue, depth: usize) -> Option<serde_json::Value> {
    if depth > 3 || value.is_null() {
        return None;
    }
    match value.kind() {
        AXValueKind::String | AXValueKind::AttributedString | AXValueKind::Url => {
            value.as_string().map(serde_json::Value::String)
        }
        AXValueKind::Bool => value.as_bool().map(serde_json::Value::Bool),
        AXValueKind::Integer => value.as_i64().map(|value| serde_json::json!(value)),
        AXValueKind::Float => value.as_f64().map(|value| serde_json::json!(value)),
        AXValueKind::Point => value.as_point().map(|point| {
            serde_json::json!({
                "x": point.x,
                "y": point.y,
            })
        }),
        AXValueKind::Size => value.as_size().map(|size| {
            serde_json::json!({
                "width": size.width,
                "height": size.height,
            })
        }),
        AXValueKind::Rect => value.as_rect().map(|rect| {
            serde_json::json!({
                "origin": { "x": rect.origin.x, "y": rect.origin.y },
                "size": { "width": rect.size.width, "height": rect.size.height },
            })
        }),
        AXValueKind::Range => value.as_range().map(|range| {
            serde_json::json!({
                "location": range.location,
                "length": range.length,
            })
        }),
        AXValueKind::Array => value.as_array().map(|items| {
            serde_json::Value::Array(
                items
                    .iter()
                    .filter_map(|item| value_to_json(item, depth + 1))
                    .take(32)
                    .collect(),
            )
        }),
        AXValueKind::Dictionary => value.as_dictionary().map(|items| {
            let values = items
                .iter()
                .filter_map(|(key, item)| value_to_json(item, depth + 1).map(|value| (key, value)))
                .take(32)
                .collect::<BTreeMap<_, _>>();
            serde_json::to_value(values).ok()
        })?,
        AXValueKind::Error | AXValueKind::Data | AXValueKind::Element => None,
        AXValueKind::TextMarker
        | AXValueKind::TextMarkerRange
        | AXValueKind::Null
        | AXValueKind::Unknown => None,
        _ => None,
    }
}

fn find_target_pid(process_name: &str, bundle_id: &str) -> Option<i32> {
    let output = Command::new("pgrep")
        .args(["-x", process_name])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<i32>().ok())
        .find(|pid| process_bundle_id(*pid).as_deref() == Some(bundle_id))
}

fn process_bundle_id(pid: i32) -> Option<String> {
    let output = Command::new("lsappinfo")
        .args(["info", "-only", "bundleID", &pid.to_string()])
        .output()
        .ok()?;
    let output = String::from_utf8_lossy(&output.stdout);
    output
        .split_once("=")
        .map(|(_, value)| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::find_target_pid;

    #[test]
    fn missing_process_is_not_an_error() {
        assert_eq!(
            find_target_pid(
                "stock-operator-process-that-does-not-exist",
                "com.example.missing"
            ),
            None
        );
    }
}
