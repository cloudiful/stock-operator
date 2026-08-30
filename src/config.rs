use std::{env, net::SocketAddr};

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
        })
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}{}", self.bind_addr, self.mcp_path)
    }
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
    use super::normalize_path;

    #[test]
    fn normalizes_mcp_path() {
        assert_eq!(normalize_path("mcp/"), "/mcp");
        assert_eq!(normalize_path("/"), "/");
    }
}
