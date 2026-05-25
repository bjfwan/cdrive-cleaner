use std::collections::HashMap;
use std::sync::Mutex;

use super::types::{AiSnapshot, AiSuggestion};

pub struct AiState {
    snapshots: Mutex<HashMap<String, StoredSnapshot>>,
    suggestions: Mutex<HashMap<String, AiSuggestion>>,
}

struct StoredSnapshot {
    snapshot: AiSnapshot,
    token_to_path: HashMap<String, String>,
}

impl AiState {
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
            suggestions: Mutex::new(HashMap::new()),
        }
    }

    pub fn store_snapshot(
        &self,
        scan_id: &str,
        snapshot: AiSnapshot,
        token_to_path: HashMap<String, String>,
    ) {
        let mut guard = self.snapshots.lock().expect("ai snapshots poisoned");
        guard.insert(
            scan_id.to_string(),
            StoredSnapshot {
                snapshot,
                token_to_path,
            },
        );
    }

    pub fn get_snapshot(&self, scan_id: &str) -> Option<AiSnapshot> {
        let guard = self.snapshots.lock().expect("ai snapshots poisoned");
        guard.get(scan_id).map(|s| s.snapshot.clone())
    }

    pub fn resolve_token(&self, token: &str) -> Option<String> {
        let guard = self.snapshots.lock().expect("ai snapshots poisoned");
        for stored in guard.values() {
            if let Some(p) = stored.token_to_path.get(token) {
                return Some(p.clone());
            }
        }
        None
    }

    pub fn store_suggestions(&self, suggestions: &[AiSuggestion]) {
        let mut guard = self.suggestions.lock().expect("ai suggestions poisoned");
        for s in suggestions {
            guard.insert(s.id.clone(), s.clone());
        }
    }

    pub fn get_suggestion(&self, id: &str) -> Option<AiSuggestion> {
        let guard = self.suggestions.lock().expect("ai suggestions poisoned");
        guard.get(id).cloned()
    }
}
