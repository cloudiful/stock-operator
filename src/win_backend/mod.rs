//! Windows-only backend for the 恒生/至胜 terminal (`xiadan.exe`).
//!
//! Task 2 scope: read-only window discovery, `Static` funds/quotes reads and the
//! health gate (`mutations_allowed`). Task 3 adds popup handling (`popups`) and
//! grid reads land in Task 4.

pub mod popups;
pub mod read;
pub mod window;

use anyhow::Result;

use crate::backend::{Backend, BackendStatus, FundsView, QuoteView, WinText};

use self::window::{MAIN_WINDOW_CLASS, MAIN_WINDOW_TITLE, RawWindow};

/// Upper bound for one read-only enumeration of the terminal window.
const MAX_SCAN_WINDOWS: usize = 4096;

#[derive(Debug, Default)]
pub struct WinBackend;

impl WinBackend {
    pub fn new() -> Self {
        Self
    }

    /// HWND of the terminal main window; `Err` when it is not running.
    pub fn find_main_window(&self) -> Result<isize> {
        window::find_main_window()
    }

    /// All `Static` controls (labels and values) with their paired label.
    ///
    /// `read_statics`/`read_funds`/`read_quotes` are the Task 2 read surface for
    /// the Task 4/5 wiring; `snapshot` already exposes the same data, and
    /// MCP/HTTP surfaces stay unchanged in this task.
    #[allow(dead_code)]
    pub fn read_statics(&self) -> Result<Vec<WinText>> {
        let handle = self.find_main_window()?;
        Ok(read::pair_label_values(&self.scan(handle)))
    }

    #[allow(dead_code)]
    pub fn read_funds(&self) -> Result<FundsView> {
        Ok(read::parse_funds(&self.read_statics()?))
    }

    #[allow(dead_code)]
    pub fn read_quotes(&self) -> Result<QuoteView> {
        Ok(read::parse_quotes(&self.read_statics()?))
    }

    fn scan(&self, handle: isize) -> Vec<RawWindow> {
        window::bounded_children(handle, MAX_SCAN_WINDOWS)
    }
}

impl Backend for WinBackend {
    fn name(&self) -> &'static str {
        "win32-hs"
    }

    fn status(&self) -> BackendStatus {
        let mut notes = Vec::new();
        let found = self.find_main_window();
        let (main_hwnd, target_pid) = match &found {
            Ok(handle) => (Some(*handle), window::main_window_pid(*handle)),
            Err(error) => {
                notes.push(format!("main window not found: {error}"));
                (None, None)
            }
        };
        let main_window_found = main_hwnd.is_some();
        let mut version_low_detected = main_hwnd
            .map(|handle| read::detect_version_low(&self.scan(handle)))
            .unwrap_or(false);
        // A blocking announcement is dismissed here, so `blocking_popup` reports
        // what is still on screen after the verified attempt, not what was.
        let mut blocking_popup = false;
        if let Some(handle) = main_hwnd {
            match popups::ensure_no_blocking_popup(handle) {
                Ok(popups::PopupOutcome::DismissedAnnouncement) => {
                    notes.push("announcement overlay dismissed (今日不再提示 + 确定)".to_string());
                }
                Ok(popups::PopupOutcome::VersionLowBlocksMutations) => {
                    version_low_detected = true;
                    notes.push("版本过低 dialog is open: live mutations stay disabled".to_string());
                }
                Ok(popups::PopupOutcome::None) => {}
                Err(error) => {
                    blocking_popup = true;
                    notes.push(format!("announcement overlay dismissal failed: {error:#}"));
                }
            }
        }
        BackendStatus {
            backend: self.name(),
            target_process_name: crate::config::DEFAULT_TARGET_PROCESS_NAME.to_string(),
            target_pid,
            target_found: main_window_found,
            main_window_found,
            version_low_detected,
            blocking_popup,
            mutations_allowed: main_window_found && !version_low_detected && !blocking_popup,
            notes,
        }
    }

    fn snapshot(&self, max_depth: usize, max_nodes: usize) -> Result<serde_json::Value> {
        let handle = self.find_main_window()?;
        let nodes = window::bounded_children(handle, max_nodes.clamp(1, MAX_SCAN_WINDOWS));
        let statics = read::pair_label_values(&nodes);
        let root_depth = window::window_depth(handle);
        let depth_limit = root_depth + max_depth.max(1);
        let visible_nodes: Vec<serde_json::Value> = nodes
            .iter()
            .filter(|node| node.depth <= depth_limit)
            .map(node_json)
            .collect();
        Ok(serde_json::json!({
            "backend": self.status(),
            "window": {
                "class": MAIN_WINDOW_CLASS,
                "title": MAIN_WINDOW_TITLE,
                "handle": handle,
                "pid": window::main_window_pid(handle),
            },
            "root_depth": root_depth,
            "depth_limit": depth_limit,
            "node_count": nodes.len(),
            "nodes": visible_nodes,
            "statics": statics,
            "funds": read::parse_funds(&statics),
            "quotes": read::parse_quotes(&statics),
        }))
    }

    fn read_texts(&self, class: &str) -> Result<Vec<WinText>> {
        let handle = self.find_main_window()?;
        let nodes = self.scan(handle);
        if class == "Static" {
            return Ok(read::pair_label_values(&nodes));
        }
        Ok(nodes
            .iter()
            .filter(|node| node.class == class)
            .map(|node| WinText {
                class: node.class.clone(),
                label: String::new(),
                value: node.text.clone(),
                handle: node.handle,
            })
            .collect())
    }
}

/// Transitional alias kept so call sites outside this task's file scope
/// (`app.rs`, `desktop/server.rs`, `mcp.rs`) construct the real backend.
pub type WinStub = WinBackend;

fn node_json(node: &RawWindow) -> serde_json::Value {
    serde_json::json!({
        "handle": node.handle,
        "parent": node.parent,
        "depth": node.depth,
        "class": node.class,
        "text": node.text,
        "visible": node.visible,
        "rect": [node.rect.left, node.rect.top, node.rect.right, node.rect.bottom],
    })
}
