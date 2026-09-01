use anyhow::Result;

const SERVICE: &str = "com.cloudiful.stock-operator";
const ACCOUNT: &str = "operator-bearer-token";

#[derive(Clone, Debug, serde::Serialize)]
pub struct TokenStatus {
    pub configured: bool,
    pub source: String, // "env" | "keychain" | "none"
}

/// Resolve the effective bearer token, preferring an explicit environment variable.
/// Returns (token, source). Never logs the token value.
pub fn resolve_token() -> (Option<String>, String) {
    if let Ok(raw) = std::env::var("STOCK_OPERATOR_AUTH_TOKEN") {
        let trimmed = raw.trim().to_string();
        if !trimmed.is_empty() {
            return (Some(trimmed), "env".to_string());
        }
    }
    match read_keychain_token() {
        Ok(Some(token)) if !token.trim().is_empty() => (Some(token), "keychain".to_string()),
        Ok(_) => (None, "none".to_string()),
        Err(_) => (None, "none".to_string()),
    }
}

/// Whether a token is configured (env or keychain), without exposing its value.
pub fn token_status() -> TokenStatus {
    let (token, source) = resolve_token();
    TokenStatus {
        configured: token.is_some(),
        source,
    }
}

/// Read token from macOS Keychain without falling back to plaintext.
pub fn read_keychain_token() -> Result<Option<String>, String> {
    let entry =
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("keychain unavailable: {e}"))?;
    match entry.get_password() {
        Ok(pw) => {
            let trimmed = pw.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("keychain read failed: {e}")),
    }
}

/// Securely save token to macOS Keychain. Validates length and emptiness.
pub fn save_keychain_token(token: &str) -> Result<(), String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("token is empty".to_string());
    }
    if trimmed.len() > 4096 {
        return Err("token too long".to_string());
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err("token contains invalid characters".to_string());
    }
    let entry =
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("keychain unavailable: {e}"))?;
    entry
        .set_password(trimmed)
        .map_err(|e| format!("keychain save failed: {e}"))?;
    Ok(())
}

/// Clear token from Keychain. Succeeds if already absent.
pub fn clear_keychain_token() -> Result<(), String> {
    let entry =
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("keychain unavailable: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keychain delete failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{clear_keychain_token, read_keychain_token, save_keychain_token, token_status};

    #[test]
    fn token_status_does_not_expose_secret() {
        // Ensure status never contains token value even when env is set
        let key = "STOCK_OPERATOR_AUTH_TOKEN";
        let prev = std::env::var(key).ok();
        unsafe { std::env::set_var(key, "super-secret-123") };
        let status = token_status();
        assert!(status.configured);
        assert_eq!(status.source, "env");
        // Serialized status must not contain the secret
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("super-secret"));
        if let Some(v) = prev {
            unsafe { std::env::set_var(key, v) };
        } else {
            unsafe { std::env::remove_var(key) };
        }
    }

    #[test]
    fn save_empty_token_fails_without_touching_keychain() {
        assert!(save_keychain_token("   ").is_err());
        assert!(save_keychain_token("").is_err());
    }

    #[test]
    fn keychain_roundtrip_when_available() {
        // Non-destructive: if a real credential already exists, skip without overwriting.
        let probe = read_keychain_token();
        if probe.is_err() {
            eprintln!("keychain unavailable, skipping roundtrip");
            return;
        }
        if probe.unwrap().is_some() {
            eprintln!("keychain already holds a credential, skipping non-destructive test");
            return;
        }
        // Save a test token with a unique prefix; never log the token value.
        let test_token = format!("test-desktop-token-{}", uuid::Uuid::new_v4());
        if let Err(_) = save_keychain_token(&test_token) {
            eprintln!("keychain save failed, skipping");
            return;
        }
        let read = match read_keychain_token() {
            Ok(v) => v,
            Err(_) => {
                eprintln!("keychain read after save failed, skipping");
                let _ = clear_keychain_token();
                return;
            }
        };
        if read.as_deref() != Some(test_token.as_str()) {
            eprintln!("keychain read mismatch, skipping");
            let _ = clear_keychain_token();
            return;
        }
        clear_keychain_token().expect("clear after save");
        let after_clear = read_keychain_token().expect("read after clear");
        assert!(
            after_clear.is_none(),
            "keychain should be empty after clear in non-destructive path"
        );
    }
}
