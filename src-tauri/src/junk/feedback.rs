use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

use crate::utils;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JunkFeedback {
    pub path: String,
    pub rule_id: String,
    pub rule_name: String,
    pub user_note: String,
    pub reported_at_iso: String,
}

pub fn append_feedback(feedback: JunkFeedback) -> std::io::Result<PathBuf> {
    let log_dir = utils::get_logs_dir()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    std::fs::create_dir_all(&log_dir)?;
    let target = log_dir.join("junk-feedback.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)?;
    let line = serde_json::to_string(&feedback)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    writeln!(file, "{}", line)?;
    file.flush()?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_serializes_to_jsonl_line() {
        let f = JunkFeedback {
            path: "C:\\Users\\X\\AppData\\Local\\Foo\\Cache".into(),
            rule_id: "foo_cache".into(),
            rule_name: "Foo 缓存".into(),
            user_note: "这个是配置不是缓存".into(),
            reported_at_iso: "2026-05-25T12:00:00Z".into(),
        };
        let s = serde_json::to_string(&f).unwrap();
        assert!(s.contains("foo_cache"));
        assert!(s.contains("这个是配置不是缓存"));
        assert!(!s.contains('\n'));
    }
}
