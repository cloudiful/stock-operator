use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::params;
use std::collections::HashMap;

use super::Storage;

const SETTING_INSTANCE_ID: &str = "operator_instance_id";
const SETTING_STOCK_SERVICE_URL: &str = "stock_main_service_url";
const SETTING_BIND_ADDR: &str = "bind_addr";
const SETTING_MCP_PATH: &str = "mcp_path";
const SETTING_TARGET_BUNDLE_ID: &str = "target_bundle_id";
const SETTING_TARGET_PROCESS_NAME: &str = "target_process_name";
const SETTING_MAX_DEPTH: &str = "max_depth";
const SETTING_MAX_NODES: &str = "max_nodes";

impl Storage {
    pub fn ensure_instance_id(&self, configured: Option<String>) -> Result<String> {
        if let Some(id) = configured {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                self.set_setting(SETTING_INSTANCE_ID, trimmed)?;
                return Ok(trimmed.to_string());
            }
        }
        if let Some(existing) = self.get_setting(SETTING_INSTANCE_ID)? {
            if !existing.trim().is_empty() {
                return Ok(existing);
            }
        }
        let new_id = uuid::Uuid::new_v4().to_string();
        self.set_setting(SETTING_INSTANCE_ID, &new_id)?;
        Ok(new_id)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM operator_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .context("get_setting query")?;
        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
            params![key, value, now],
        )
        .with_context(|| format!("failed to set setting {key}"))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn list_settings(&self) -> Result<HashMap<String, String>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT key, value FROM operator_settings")
            .context("prepare list_settings")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .context("query list_settings")?
            .collect::<Result<HashMap<_, _>, _>>()
            .context("collect settings")?;
        Ok(rows)
    }

    pub fn seed_from_config(
        &self,
        stock_service_url: Option<&str>,
        bind_addr: &str,
        mcp_path: &str,
        target_bundle_id: &str,
        target_process_name: &str,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<()> {
        if self.get_setting(SETTING_BIND_ADDR)?.is_none() {
            self.set_setting(SETTING_BIND_ADDR, bind_addr)?;
        }
        if self.get_setting(SETTING_MCP_PATH)?.is_none() {
            self.set_setting(SETTING_MCP_PATH, mcp_path)?;
        }
        if self.get_setting(SETTING_TARGET_BUNDLE_ID)?.is_none() {
            self.set_setting(SETTING_TARGET_BUNDLE_ID, target_bundle_id)?;
        }
        if self.get_setting(SETTING_TARGET_PROCESS_NAME)?.is_none() {
            self.set_setting(SETTING_TARGET_PROCESS_NAME, target_process_name)?;
        }
        if self.get_setting(SETTING_MAX_DEPTH)?.is_none() {
            self.set_setting(SETTING_MAX_DEPTH, &max_depth.to_string())?;
        }
        if self.get_setting(SETTING_MAX_NODES)?.is_none() {
            self.set_setting(SETTING_MAX_NODES, &max_nodes.to_string())?;
        }
        if let Some(url) = stock_service_url {
            if !url.trim().is_empty() && self.get_setting(SETTING_STOCK_SERVICE_URL)?.is_none() {
                self.set_setting(SETTING_STOCK_SERVICE_URL, url.trim())?;
            }
        }
        Ok(())
    }
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e).context("query optional"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Storage;

    #[test]
    fn settings_instance_id_generated_and_persisted() {
        let storage = Storage::open_in_memory().unwrap();
        let first = storage.ensure_instance_id(None).unwrap();
        assert!(!first.is_empty());
        let second = storage.ensure_instance_id(None).unwrap();
        assert_eq!(first, second);
        let configured = storage
            .ensure_instance_id(Some("custom-id-123".to_string()))
            .unwrap();
        assert_eq!(configured, "custom-id-123");
        assert_eq!(
            storage
                .get_setting(super::SETTING_INSTANCE_ID)
                .unwrap()
                .unwrap(),
            "custom-id-123"
        );
    }
}
