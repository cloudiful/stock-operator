use std::{
    env,
    path::{Path, PathBuf},
};

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

pub(crate) fn bounded_env_usize(key: &str, default: usize, min: usize, max: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}
