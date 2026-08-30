use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:5190";
const DEFAULT_MCP_PATH: &str = "/mcp";
const DEFAULT_TARGET_BUNDLE_ID: &str = "com.citics.mac.tdx";
const DEFAULT_TARGET_PROCESS_NAME: &str = "中信证券网上交易";
const DEFAULT_MAX_DEPTH: usize = 6;
const DEFAULT_MAX_NODES: usize = 300;

#[derive(Clone, Debug)]
pub struct OperatorConfig {
    pub bind_addr: SocketAddr,
    pub mcp_path: String,
    pub auth_token: Option<String>,
    pub target_bundle_id: String,
    pub target_process_name: String,
    pub max_depth: usize,
    pub max_nodes: usize,
    /// Resolved database path. Env override STOCK_OPERATOR_DB_PATH, default
    /// ~/Library/Application Support/Stock Operator/operator.sqlite3 on macOS.
    pub db_path: PathBuf,
    /// Stock main-service URL for Tauri UI / bridge. Optional, never a secret.
    pub stock_service_url: Option<String>,
    /// Configured instance identifier, if provided via env. Generated otherwise
    /// and persisted in SQLite as `operator_instance_id`.
    pub instance_id: Option<String>,
}

impl OperatorConfig {
    pub fn from_env() -> Result<Self> {
        let bind_addr = env::var("STOCK_OPERATOR_BIND_ADDR")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
            .parse::<SocketAddr>()
            .context("STOCK_OPERATOR_BIND_ADDR must be a socket address")?;
        if !bind_addr.ip().is_loopback() {
            bail!("stock-operator only binds to loopback; use a 127.0.0.1 or ::1 address");
        }

        let auth_token = env::var("STOCK_OPERATOR_AUTH_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty());

        let db_path = resolve_db_path();
        let stock_service_url = env::var("STOCK_OPERATOR_MAIN_SERVICE_URL")
            .or_else(|_| env::var("STOCK_OPERATOR_STOCK_SERVICE_URL"))
            .or_else(|_| env::var("STOCK_MAIN_SERVICE_URL"))
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let instance_id = env::var("STOCK_OPERATOR_INSTANCE_ID")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        Ok(Self {
            bind_addr,
            mcp_path: normalize_path(
                &env::var("STOCK_OPERATOR_MCP_PATH")
                    .unwrap_or_else(|_| DEFAULT_MCP_PATH.to_string()),
            ),
            auth_token,
            target_bundle_id: env::var("STOCK_OPERATOR_TARGET_BUNDLE_ID")
                .unwrap_or_else(|_| DEFAULT_TARGET_BUNDLE_ID.to_string()),
            target_process_name: env::var("STOCK_OPERATOR_TARGET_PROCESS_NAME")
                .unwrap_or_else(|_| DEFAULT_TARGET_PROCESS_NAME.to_string()),
            max_depth: bounded_env_usize("STOCK_OPERATOR_MAX_DEPTH", DEFAULT_MAX_DEPTH, 1, 12),
            max_nodes: bounded_env_usize("STOCK_OPERATOR_MAX_NODES", DEFAULT_MAX_NODES, 1, 2_000),
            db_path,
            stock_service_url,
            instance_id,
        })
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}{}", self.bind_addr, self.mcp_path)
    }

    /// Human-readable database location for diagnostics.
    pub fn db_path_display(&self) -> String {
        self.db_path.display().to_string()
    }
}

pub fn resolve_db_path() -> PathBuf {
    if let Ok(overridden) = env::var("STOCK_OPERATOR_DB_PATH") {
        let trimmed = overridden.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    default_db_path()
}

pub fn default_db_path() -> PathBuf {
    // macOS per-user Application Support
    if let Ok(home) = env::var("HOME") {
        if !home.trim().is_empty() {
            return Path::new(&home)
                .join("Library")
                .join("Application Support")
                .join("Stock Operator")
                .join("operator.sqlite3");
        }
    }
    // Fallback to XDG or current directory for tests/CI outside macOS
    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            return Path::new(&xdg)
                .join("stock-operator")
                .join("operator.sqlite3");
        }
    }
    PathBuf::from("operator.sqlite3")
}

fn normalize_path(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value == "/" {
        return "/".to_string();
    }
    format!("/{}", value.trim_matches('/'))
}

fn bounded_env_usize(key: &str, default: usize, min: usize, max: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::{default_db_path, normalize_path, resolve_db_path};
    use std::env;

    #[test]
    fn normalizes_mcp_path() {
        assert_eq!(normalize_path("mcp/"), "/mcp");
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn default_db_path_is_application_support() {
        let path = default_db_path();
        let s = path.to_string_lossy();
        assert!(s.contains("operator.sqlite3"));
        // On CI container HOME may be /root or /Users/...
        assert!(
            s.contains("Stock Operator") || s == "operator.sqlite3" || s.contains("stock-operator")
        );
    }

    #[test]
    fn env_override_takes_precedence_for_db_path() {
        let key = "STOCK_OPERATOR_DB_PATH";
        let prev = env::var(key).ok();
        unsafe { env::set_var(key, "/tmp/test-operator.sqlite3") };
        assert_eq!(
            resolve_db_path(),
            std::path::PathBuf::from("/tmp/test-operator.sqlite3")
        );
        if let Some(v) = prev {
            unsafe { env::set_var(key, v) };
        } else {
            unsafe { env::remove_var(key) };
        }
    }

    #[test]
    fn auth_token_not_in_default_settings() {
        // Ensure config does not set token-related env as db path
        let cfg = super::OperatorConfig::from_env().unwrap();
        let display = cfg.db_path_display();
        assert!(!display.contains("token"));
    }
}
