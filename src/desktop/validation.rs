use std::net::SocketAddr;

use crate::config::{NetworkMode, validate_bind_for_mode};

fn normalize_path(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value == "/" {
        return "/".to_string();
    }
    format!("/{}", value.trim_matches('/'))
}

#[allow(dead_code)]
pub fn validate_bind_addr(addr: &str) -> Result<SocketAddr, String> {
    validate_bind_addr_for_mode(addr, &NetworkMode::Loopback.to_string())
}

pub fn validate_bind_addr_for_mode(addr: &str, mode_raw: &str) -> Result<SocketAddr, String> {
    let trimmed = addr.trim();
    if trimmed.is_empty() {
        return Err("bind address is required".to_string());
    }
    let mode = NetworkMode::parse(mode_raw).map_err(|e| format!("network_mode: {e}"))?;
    let parsed: SocketAddr = trimmed
        .parse()
        .map_err(|_| "bind address must be a socket address like 127.0.0.1:5190".to_string())?;
    validate_bind_for_mode(parsed, mode).map_err(|e| e.to_string())?;
    Ok(parsed)
}

pub fn validate_network_mode(raw: &str) -> Result<String, String> {
    let mode = NetworkMode::parse(raw).map_err(|e| e)?;
    Ok(mode.as_str().to_string())
}

#[allow(dead_code)]
pub fn requires_private_ack(mode_raw: &str, bind_raw: &str) -> bool {
    let Ok(mode) = NetworkMode::parse(mode_raw) else {
        return false;
    };
    if mode == NetworkMode::PrivateOverlay {
        return true;
    }
    // Non-loopback in loopback mode would already be rejected, but treat as requiring ack if it somehow passed.
    if let Ok(addr) = bind_raw.trim().parse::<SocketAddr>() {
        return !addr.ip().is_loopback();
    }
    false
}

pub fn validate_mcp_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("MCP path is required".to_string());
    }
    let normalized = normalize_path(trimmed);
    if normalized.len() > 128 {
        return Err("MCP path too long".to_string());
    }
    if normalized.contains(' ') || normalized.contains('\\') {
        return Err("MCP path contains invalid characters".to_string());
    }
    Ok(normalized)
}

pub fn validate_stock_service_url(url: &Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = url else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > 2048 {
        return Err("stock service URL too long".to_string());
    }
    let parsed = url::Url::parse(trimmed).map_err(|e| format!("invalid URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("stock service URL must be http or https".to_string()),
    }
    if parsed.host_str().is_none() {
        return Err("stock service URL must have a host".to_string());
    }
    Ok(Some(trimmed.to_string()))
}

pub fn validate_bundle_id(id: &str) -> Result<String, String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err("bundle identifier is required".to_string());
    }
    if trimmed.len() > 256 {
        return Err("bundle identifier too long".to_string());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        return Err("bundle identifier contains invalid characters".to_string());
    }
    if !trimmed.contains('.') {
        return Err("bundle identifier should contain a dot".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn validate_process_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("process name is required".to_string());
    }
    if trimmed.len() > 256 {
        return Err("process name too long".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn validate_max_depth(v: usize) -> Result<usize, String> {
    if !(1..=12).contains(&v) {
        return Err("max depth must be between 1 and 12".to_string());
    }
    Ok(v)
}

pub fn validate_max_nodes(v: usize) -> Result<usize, String> {
    if !(1..=2000).contains(&v) {
        return Err("max nodes must be between 1 and 2000".to_string());
    }
    Ok(v)
}

// Re-export helper for tests that want direct ip check
#[allow(dead_code, unused_imports)]
pub use crate::config::is_private_ip as is_private_network_ip;

#[cfg(test)]
mod tests {
    use super::{
        is_private_network_ip, validate_bind_addr, validate_bind_addr_for_mode, validate_bundle_id,
        validate_mcp_path, validate_network_mode, validate_stock_service_url,
    };

    #[test]
    fn validates_bind_addr_loopback_only() {
        assert!(validate_bind_addr("127.0.0.1:5190").is_ok());
        assert!(validate_bind_addr("[::1]:5190").is_ok());
        assert!(validate_bind_addr("0.0.0.0:5190").is_err());
        assert!(validate_bind_addr("192.168.1.10:5190").is_err());
        assert!(validate_bind_addr("not-an-addr").is_err());
    }

    #[test]
    fn private_overlay_allows_private_rejects_public() {
        assert!(validate_bind_addr_for_mode("192.168.1.10:5190", "private-overlay").is_ok());
        assert!(validate_bind_addr_for_mode("10.0.0.5:5190", "private").is_ok());
        assert!(validate_bind_addr_for_mode("100.64.0.5:5190", "private-overlay").is_ok());
        assert!(validate_bind_addr_for_mode("100.127.5.1:5190", "private-overlay").is_ok());
        assert!(validate_bind_addr_for_mode("8.8.8.8:5190", "private-overlay").is_err());
        assert!(validate_bind_addr_for_mode("0.0.0.0:5190", "private-overlay").is_err());
        assert!(validate_bind_addr_for_mode("::0:5190", "private-overlay").is_err());
    }

    #[test]
    fn validates_network_mode() {
        assert_eq!(validate_network_mode("loopback").unwrap(), "loopback");
        assert_eq!(
            validate_network_mode("private-overlay").unwrap(),
            "private-overlay"
        );
        assert_eq!(validate_network_mode("private").unwrap(), "private-overlay");
        assert!(validate_network_mode("public").is_err());
    }

    #[test]
    fn validates_mcp_path() {
        assert_eq!(validate_mcp_path("/mcp").unwrap(), "/mcp");
        assert_eq!(validate_mcp_path("mcp/").unwrap(), "/mcp");
        assert!(validate_mcp_path("").is_err());
        assert!(validate_mcp_path("/a b").is_err());
    }

    #[test]
    fn validates_stock_url() {
        assert!(validate_stock_service_url(&Some("http://localhost:3000".to_string())).is_ok());
        assert!(validate_stock_service_url(&Some("https://example.com/api".to_string())).is_ok());
        assert!(validate_stock_service_url(&Some("ftp://example.com".to_string())).is_err());
        assert!(validate_stock_service_url(&Some("not a url".to_string())).is_err());
        assert!(validate_stock_service_url(&None).unwrap().is_none());
        assert!(
            validate_stock_service_url(&Some("".to_string()))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn validates_bundle_id() {
        assert!(validate_bundle_id("com.citics.mac.tdx").is_ok());
        assert!(validate_bundle_id("").is_err());
        assert!(validate_bundle_id("no-dot").is_err());
        assert!(validate_bundle_id("com example").is_err());
    }

    #[test]
    fn private_ip_detection() {
        assert!(is_private_network_ip("192.168.1.1".parse().unwrap()));
        assert!(is_private_network_ip("10.5.6.7".parse().unwrap()));
        assert!(is_private_network_ip("100.64.0.1".parse().unwrap()));
        assert!(!is_private_network_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_private_network_ip("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn loopback_always_allowed_private_mode_also() {
        assert!(validate_bind_addr_for_mode("127.0.0.1:5190", "loopback").is_ok());
        assert!(validate_bind_addr_for_mode("127.0.0.1:5190", "private-overlay").is_ok());
        assert!(validate_bind_addr_for_mode("[::1]:5190", "loopback").is_ok());
        assert!(validate_bind_addr_for_mode("[::1]:5190", "private-overlay").is_ok());
    }
}
