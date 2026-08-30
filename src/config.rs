use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:5190";
const DEFAULT_MCP_PATH: &str = "/mcp";
const DEFAULT_TARGET_BUNDLE_ID: &str = "com.citics.mac.tdx";
const DEFAULT_TARGET_PROCESS_NAME: &str = "中信证券网上交易";
const DEFAULT_MAX_DEPTH: usize = 6;
const DEFAULT_MAX_NODES: usize = 300;
const DEFAULT_NETWORK_MODE: &str = "loopback";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkMode {
    Loopback,
    PrivateOverlay,
}

impl NetworkMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::PrivateOverlay => "private-overlay",
        }
    }
    pub fn parse(raw: &str) -> Result<Self, String> {
        let v = raw.trim().to_ascii_lowercase();
        match v.as_str() {
            "loopback" | "loopback-only" | "loopback_only" => Ok(Self::Loopback),
            "private" | "private-overlay" | "private_overlay" | "overlay" | "tailscale"
            | "wireguard" => Ok(Self::PrivateOverlay),
            "" => Err("network mode is required".to_string()),
            _ => Err(format!(
                "unknown network mode '{}'; expected 'loopback' or 'private-overlay'",
                raw.trim()
            )),
        }
    }
}
impl std::fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl Default for NetworkMode {
    fn default() -> Self {
        Self::Loopback
    }
}

#[derive(Clone, Debug)]
pub struct OperatorConfig {
    pub bind_addr: SocketAddr,
    pub mcp_path: String,
    pub auth_token: Option<String>,
    pub target_bundle_id: String,
    pub target_process_name: String,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub db_path: PathBuf,
    pub stock_service_url: Option<String>,
    pub instance_id: Option<String>,
    pub network_mode: NetworkMode,
}

impl OperatorConfig {
    pub fn from_env() -> Result<Self> {
        let network_mode = resolve_network_mode_env()?;
        let bind_addr = env::var("STOCK_OPERATOR_BIND_ADDR")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
            .parse::<SocketAddr>()
            .context("STOCK_OPERATOR_BIND_ADDR must be a socket address")?;
        validate_bind_for_mode(bind_addr, network_mode).map_err(|e| anyhow::anyhow!(e))?;

        let auth_token = env::var("STOCK_OPERATOR_AUTH_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty());

        // Private overlay with non-loopback requires bearer auth at startup (env).
        if network_mode == NetworkMode::PrivateOverlay
            && !bind_addr.ip().is_loopback()
            && auth_token.is_none()
        {
            bail!(
                "private-overlay network mode with non-loopback bind requires STOCK_OPERATOR_AUTH_TOKEN (or Keychain token when running desktop)"
            );
        }

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
            network_mode,
        })
    }

    pub fn from_env_with_storage(storage: &crate::storage::Storage) -> Result<Self> {
        let db_path = resolve_db_path();
        let network_mode = resolve_network_mode_with_storage(storage)?;

        let bind_addr = if let Ok(raw) = env::var("STOCK_OPERATOR_BIND_ADDR") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                let addr: SocketAddr = trimmed
                    .parse()
                    .context("STOCK_OPERATOR_BIND_ADDR must be a socket address")?;
                validate_bind_for_mode(addr, network_mode).map_err(|e| anyhow::anyhow!(e))?;
                addr
            } else {
                Self::bind_addr_from_storage(storage, network_mode)?
            }
        } else {
            Self::bind_addr_from_storage(storage, network_mode)?
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
            network_mode,
        })
    }

    fn bind_addr_from_storage(
        storage: &crate::storage::Storage,
        mode: NetworkMode,
    ) -> Result<SocketAddr> {
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
            validate_bind_for_mode(addr, mode).map_err(|e| anyhow::anyhow!(e))?;
            return Ok(addr);
        }
        let def: SocketAddr = DEFAULT_BIND_ADDR.parse().unwrap();
        validate_bind_for_mode(def, mode).map_err(|e| anyhow::anyhow!(e))?;
        Ok(def)
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

    pub fn db_path_display(&self) -> String {
        self.db_path.display().to_string()
    }
}

fn resolve_network_mode_env() -> Result<NetworkMode> {
    if let Ok(raw) = env::var("STOCK_OPERATOR_NETWORK_MODE") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return NetworkMode::parse(trimmed).map_err(|e| anyhow::anyhow!(e));
        }
    }
    Ok(NetworkMode::Loopback)
}

fn resolve_network_mode_with_storage(storage: &crate::storage::Storage) -> Result<NetworkMode> {
    if let Ok(raw) = env::var("STOCK_OPERATOR_NETWORK_MODE") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return NetworkMode::parse(trimmed).map_err(|e| anyhow::anyhow!(e));
        }
    }
    if let Some(raw) = storage
        .get_setting("network_mode")
        .ok()
        .flatten()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return NetworkMode::parse(&raw).map_err(|e| anyhow::anyhow!(e));
    }
    Ok(NetworkMode::Loopback)
}

pub fn validate_bind_for_mode(addr: SocketAddr, mode: NetworkMode) -> Result<(), String> {
    let ip = addr.ip();
    if ip.is_unspecified() {
        return Err(format!(
            "bind address {addr} is unspecified (0.0.0.0 or ::); public exposure is not allowed"
        ));
    }
    if ip.is_loopback() {
        return Ok(());
    }
    match mode {
        NetworkMode::Loopback => Err(format!(
            "bind address {addr} is not loopback; loopback-only mode requires 127.0.0.1 or ::1. Set STOCK_OPERATOR_NETWORK_MODE=private-overlay for private network use"
        )),
        NetworkMode::PrivateOverlay => {
            if is_private_ip(ip) {
                Ok(())
            } else {
                Err(format!(
                    "bind address {addr} is not a private network address; private-overlay mode permits only loopback or private addresses (RFC1918 10/8,172.16/12,192.168/16, CGNAT 100.64/10, ULA fc00::/7, link-local). Public addresses are rejected"
                ))
            }
        }
    }
}

pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_private() || v4.is_link_local() {
                return true;
            }
            // CGNAT 100.64.0.0/10 and Tailscale range
            let octets = v4.octets();
            if octets[0] == 100 && (64..=127).contains(&octets[1]) {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return false;
            }
            v6.is_unique_local() || v6.is_unicast_link_local()
        }
    }
}

// Allow checking private for socket check without exposing Ipv4
#[allow(dead_code)]
pub fn is_cgnat(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    o[0] == 100 && (64..=127).contains(&o[1])
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
    if let Ok(home) = env::var("HOME") {
        if !home.trim().is_empty() {
            return Path::new(&home)
                .join("Library")
                .join("Application Support")
                .join("Stock Operator")
                .join("operator.sqlite3");
        }
    }
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
    use super::{
        NetworkMode, default_db_path, is_private_ip, normalize_path, resolve_db_path,
        validate_bind_for_mode,
    };
    use std::{env, net::SocketAddr};

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

    #[test]
    fn network_mode_parse() {
        assert_eq!(
            NetworkMode::parse("loopback").unwrap(),
            NetworkMode::Loopback
        );
        assert_eq!(
            NetworkMode::parse("private-overlay").unwrap(),
            NetworkMode::PrivateOverlay
        );
        assert_eq!(
            NetworkMode::parse("private").unwrap(),
            NetworkMode::PrivateOverlay
        );
        assert!(NetworkMode::parse("public").is_err());
    }

    #[test]
    fn validate_loopback_only_rejects_private_without_mode() {
        let private: SocketAddr = "192.168.1.10:5190".parse().unwrap();
        assert!(validate_bind_for_mode(private, NetworkMode::Loopback).is_err());
        assert!(validate_bind_for_mode(private, NetworkMode::PrivateOverlay).is_ok());
    }

    #[test]
    fn rejects_unspecified_and_public_even_in_private_mode() {
        let unspecified: SocketAddr = "0.0.0.0:5190".parse().unwrap();
        assert!(validate_bind_for_mode(unspecified, NetworkMode::PrivateOverlay).is_err());
        let public: SocketAddr = "8.8.8.8:5190".parse().unwrap();
        assert!(validate_bind_for_mode(public, NetworkMode::PrivateOverlay).is_err());
        let tailscale: SocketAddr = "100.64.0.5:5190".parse().unwrap();
        assert!(validate_bind_for_mode(tailscale, NetworkMode::PrivateOverlay).is_ok());
        assert!(is_private_ip("100.64.0.5".parse().unwrap()));
    }

    #[test]
    fn loopback_always_allowed() {
        let lo: SocketAddr = "127.0.0.1:5190".parse().unwrap();
        assert!(validate_bind_for_mode(lo, NetworkMode::Loopback).is_ok());
        assert!(validate_bind_for_mode(lo, NetworkMode::PrivateOverlay).is_ok());
        let lo6: SocketAddr = "[::1]:5190".parse().unwrap();
        assert!(validate_bind_for_mode(lo6, NetworkMode::Loopback).is_ok());
    }
}
