use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use anyhow::Result;

use crate::storage::Storage;

/// Default network mode string; kept for documentation and tests.
#[allow(dead_code)]
pub const DEFAULT_NETWORK_MODE: &str = "loopback";

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

#[allow(dead_code)]
pub(crate) fn resolve_network_mode_env() -> Result<NetworkMode> {
    if let Ok(raw) = env::var("STOCK_OPERATOR_NETWORK_MODE") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return NetworkMode::parse(trimmed).map_err(|e| anyhow::anyhow!(e));
        }
    }
    Ok(NetworkMode::Loopback)
}

pub(crate) fn resolve_network_mode_with_storage(storage: &Storage) -> Result<NetworkMode> {
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

#[allow(dead_code)]
pub fn is_cgnat(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    o[0] == 100 && (64..=127).contains(&o[1])
}
