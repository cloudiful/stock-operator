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

    /// Load effective config preferring explicit env vars, then persisted SQLite settings,
    /// then built-in defaults. `auth_token` remains env-only here; keychain resolution
    /// is handled by the desktop layer and merged afterwards.
    pub fn from_env_with_storage(storage: &crate::storage::Storage) -> Result<Self> {
        let db_path = resolve_db_path();

        let bind_addr = if let Ok(raw) = env::var("STOCK_OPERATOR_BIND_ADDR") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                let addr: SocketAddr = trimmed
                    .parse()
                    .context("STOCK_OPERATOR_BIND_ADDR must be a socket address")?;
                if !addr.ip().is_loopback() {
                    bail!("stock-operator only binds to loopback; use a 127.0.0.1 or ::1 address");
                }
                addr
            } else {
                Self::bind_addr_from_storage(storage)?
            }
        } else {
            Self::bind_addr_from_storage(storage)?
        };

        let mcp_path = if let Ok(raw) = env::var("STOCK_OPERATOR_MCP_PATH") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                normalize_path(trimmed)
            } else {
                Self::mcp_path_from_storage(storage)
            }
        } else {
            Self::mcp_path_from_storage(storage)
        };

        let target_bundle_id = if let Ok(raw) = env::var("STOCK_OPERATOR_TARGET_BUNDLE_ID") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                Self::bundle_id_from_storage(storage)
            }
        } else {
            Self::bundle_id_from_storage(storage)
        };

        let target_process_name = if let Ok(raw) = env::var("STOCK_OPERATOR_TARGET_PROCESS_NAME") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                Self::process_name_from_storage(storage)
            }
        } else {
            Self::process_name_from_storage(storage)
        };

        let max_depth = if let Ok(raw) = env::var("STOCK_OPERATOR_MAX_DEPTH") {
            if let Ok(v) = raw.trim().parse::<usize>() {
                v.clamp(1, 12)
            } else {
                Self::max_depth_from_storage(storage)
            }
        } else {
            Self::max_depth_from_storage(storage)
        };

        let max_nodes = if let Ok(raw) = env::var("STOCK_OPERATOR_MAX_NODES") {
            if let Ok(v) = raw.trim().parse::<usize>() {
                v.clamp(1, 2_000)
            } else {
                Self::max_nodes_from_storage(storage)
            }
        } else {
            Self::max_nodes_from_storage(storage)
        };

        let stock_service_url = env::var("STOCK_OPERATOR_MAIN_SERVICE_URL")
            .or_else(|_| env::var("STOCK_OPERATOR_STOCK_SERVICE_URL"))
            .or_else(|_| env::var("STOCK_MAIN_SERVICE_URL"))
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| {
                storage
                    .get_setting("stock_main_service_url")
                    .ok()
                    .flatten()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            });

        let auth_token = env::var("STOCK_OPERATOR_AUTH_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let instance_id = env::var("STOCK_OPERATOR_INSTANCE_ID")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| {
                storage
                    .get_setting("operator_instance_id")
                    .ok()
                    .flatten()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            });

        Ok(Self {
            bind_addr,
            mcp_path,
            auth_token,
            target_bundle_id,
            target_process_name,
            max_depth,
            max_nodes,
            db_path,
            stock_service_url,
            instance_id,
        })
    }

    fn bind_addr_from_storage(storage: &crate::storage::Storage) -> Result<SocketAddr> {
        if let Some(raw) = storage
            .get_setting("bind_addr")
            .ok()
            .flatten()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            let addr: SocketAddr = raw
                .parse()
                .context("persisted bind_addr must be a socket address")?;
            if !addr.ip().is_loopback() {
                bail!("persisted bind_addr must be loopback");
            }
            return Ok(addr);
        }
        Ok(DEFAULT_BIND_ADDR.parse().unwrap())
    }

    fn mcp_path_from_storage(storage: &crate::storage::Storage) -> String {
        if let Some(raw) = storage
            .get_setting("mcp_path")
            .ok()
            .flatten()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            normalize_path(&raw)
        } else {
            DEFAULT_MCP_PATH.to_string()
        }
    }

    fn bundle_id_from_storage(storage: &crate::storage::Storage) -> String {
        storage
            .get_setting("target_bundle_id")
            .ok()
            .flatten()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_TARGET_BUNDLE_ID.to_string())
    }

    fn process_name_from_storage(storage: &crate::storage::Storage) -> String {
        storage
            .get_setting("target_process_name")
            .ok()
            .flatten()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_TARGET_PROCESS_NAME.to_string())
    }

    fn max_depth_from_storage(storage: &crate::storage::Storage) -> usize {
        storage
            .get_setting("max_depth")
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_DEPTH)
            .clamp(1, 12)
    }

    fn max_nodes_from_storage(storage: &crate::storage::Storage) -> usize {
        storage
            .get_setting("max_nodes")
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_NODES)
            .clamp(1, 2_000)
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

pub(crate) fn normalize_path(value: &str) -> String {
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
        let cfg = super::OperatorConfig::from_env().unwrap();
        let display = cfg.db_path_display();
        assert!(!display.contains("token"));
    }
}
