use std::net::SocketAddr;

fn normalize_path(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value == "/" {
        return "/".to_string();
    }
    format!("/{}", value.trim_matches('/'))
}

pub fn validate_bind_addr(addr: &str) -> Result<SocketAddr, String> {
    let trimmed = addr.trim();
    if trimmed.is_empty() {
        return Err("bind address is required".to_string());
    }
    let parsed: SocketAddr = trimmed
        .parse()
        .map_err(|_| "bind address must be a socket address like 127.0.0.1:5190".to_string())?;
    if !parsed.ip().is_loopback() {
        return Err("bind address must be loopback (127.0.0.1 or ::1)".to_string());
    }
    Ok(parsed)
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

#[cfg(test)]
mod tests {
    use super::{
        validate_bind_addr, validate_bundle_id, validate_mcp_path, validate_stock_service_url,
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
}
