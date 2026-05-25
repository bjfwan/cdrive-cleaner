use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::utils::get_app_data_dir;

pub struct SettingsStore {
    inner: Mutex<Value>,
    path: PathBuf,
}

impl SettingsStore {
    pub fn load() -> Result<Self> {
        let path = settings_path()?;
        let value = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| anyhow!("read settings.json failed: {e}"))?;
            serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };
        let value = ensure_object(value);
        Ok(Self {
            inner: Mutex::new(value),
            path,
        })
    }

    pub fn get(&self, dotted: &str) -> Value {
        let guard = self.inner.lock().expect("settings poisoned");
        get_path(&guard, dotted).cloned().unwrap_or(Value::Null)
    }

    pub fn set(&self, dotted: &str, value: Value) -> Result<()> {
        {
            let mut guard = self.inner.lock().expect("settings poisoned");
            set_path(&mut guard, dotted, value)?;
        }
        self.persist()
    }

    pub fn snapshot(&self) -> Value {
        self.inner.lock().expect("settings poisoned").clone()
    }

    fn persist(&self) -> Result<()> {
        let snapshot = self.snapshot();
        let serialized = serde_json::to_string_pretty(&snapshot)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow!("create settings dir failed: {e}"))?;
        }
        std::fs::write(&self.path, serialized)
            .map_err(|e| anyhow!("write settings.json failed: {e}"))?;
        Ok(())
    }
}

fn settings_path() -> Result<PathBuf> {
    Ok(get_app_data_dir()?.join("settings.json"))
}

fn ensure_object(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        json!({})
    }
}

fn get_path<'a>(root: &'a Value, dotted: &str) -> Option<&'a Value> {
    let mut cur = root;
    if dotted.trim().is_empty() {
        return Some(cur);
    }
    for segment in dotted.split('.') {
        cur = cur.get(segment)?;
    }
    Some(cur)
}

fn set_path(root: &mut Value, dotted: &str, value: Value) -> Result<()> {
    if dotted.trim().is_empty() {
        return Err(anyhow!("settings path cannot be empty"));
    }
    if !root.is_object() {
        *root = json!({});
    }
    let parts: Vec<&str> = dotted.split('.').collect();
    let mut cur = root;
    for (i, segment) in parts.iter().enumerate() {
        let obj = cur
            .as_object_mut()
            .ok_or_else(|| anyhow!("settings path collides with non-object at '{segment}'"))?;
        if i + 1 == parts.len() {
            obj.insert((*segment).to_string(), value);
            return Ok(());
        }
        if !obj.get(*segment).map(|v| v.is_object()).unwrap_or(false) {
            obj.insert((*segment).to_string(), json!({}));
        }
        cur = obj.get_mut(*segment).unwrap();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_set_creates_path() {
        let mut v = json!({});
        set_path(&mut v, "ai.enabled", json!(true)).unwrap();
        assert_eq!(v["ai"]["enabled"], json!(true));
    }

    #[test]
    fn deep_set_preserves_siblings() {
        let mut v = json!({"ai": {"enabled": true, "provider": {"model": "x"}}});
        set_path(&mut v, "ai.provider.api_key", json!("k")).unwrap();
        assert_eq!(v["ai"]["enabled"], json!(true));
        assert_eq!(v["ai"]["provider"]["model"], json!("x"));
        assert_eq!(v["ai"]["provider"]["api_key"], json!("k"));
    }

    #[test]
    fn get_unknown_returns_null() {
        let v = json!({});
        assert!(get_path(&v, "missing").is_none());
    }
}
