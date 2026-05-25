use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::scanner::DiskScanner;

use super::client;
use super::sanitizer;
use super::settings::SettingsStore;
use super::state::AiState;
use super::types::{AiSettings, AiSnapshot, AiSuggestion};

const SUGGESTIONS_EVENT: &str = "ai-suggestions-ready";

#[derive(Clone, serde::Serialize)]
struct AiSuggestionsEvent {
    scan_id: String,
    suggestions: Vec<AiSuggestion>,
}

#[derive(Clone, serde::Serialize)]
struct AiErrorEvent {
    scan_id: String,
    message: String,
}

fn read_ai_settings(settings: &SettingsStore) -> AiSettings {
    let value = settings.get("ai");
    if value.is_null() {
        return AiSettings::default();
    }
    serde_json::from_value(value).unwrap_or_default()
}

fn disk_usage_for(root_path: &str) -> (u64, u64) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let drive_root = if root_path.len() >= 2 && root_path.chars().nth(1) == Some(':') {
            format!("{}:\\", &root_path[..1])
        } else {
            root_path.to_string()
        };
        let wide: Vec<u16> = std::ffi::OsStr::new(&drive_root)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut total: u64 = 0;
        let mut free: u64 = 0;
        unsafe {
            if GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), None, Some(&mut total), Some(&mut free))
                .is_ok()
            {
                return (total, free);
            }
        }
    }
    (0, 0)
}

#[tauri::command]
pub fn cmd_ai_preview_snapshot(
    scan_id: String,
    scanner: State<'_, DiskScanner>,
    ai_state: State<'_, Arc<AiState>>,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<AiSnapshot, String> {
    let scan = scanner
        .get_directory_snapshot(&scan_id, &scan_id)
        .ok_or_else(|| format!("no scan result for scan_id={scan_id}"))?;
    let (total, free) = disk_usage_for(&scan_id);
    let ai_settings = read_ai_settings(&settings);
    let result = sanitizer::build_snapshot(&scan, total, free, &ai_settings);
    ai_state.store_snapshot(&scan_id, result.snapshot.clone(), result.token_to_path);
    Ok(result.snapshot)
}

#[tauri::command]
pub async fn cmd_ai_analyze(
    scan_id: String,
    app: AppHandle,
    scanner: State<'_, DiskScanner>,
    ai_state: State<'_, Arc<AiState>>,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<(), String> {
    let scan = scanner
        .get_directory_snapshot(&scan_id, &scan_id)
        .ok_or_else(|| format!("no scan result for scan_id={scan_id}"))?;
    let (total, free) = disk_usage_for(&scan_id);
    let ai_settings = read_ai_settings(&settings);

    if !ai_settings.categories.any_enabled() {
        return Err("no ai categories enabled".into());
    }

    let prepared = sanitizer::build_snapshot(&scan, total, free, &ai_settings);
    ai_state.store_snapshot(
        &scan_id,
        prepared.snapshot.clone(),
        prepared.token_to_path,
    );

    let snapshot = prepared.snapshot;
    let scan_id_for_task = scan_id.clone();
    let ai_state_for_task = ai_state.inner().clone();
    let app_for_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let analyze_result = tokio::task::spawn_blocking(move || {
            client::analyze(&snapshot, &ai_settings)
        })
        .await;

        let suggestions = match analyze_result {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                tracing::warn!("[ai] analyze failed: {e}");
                let _ = app_for_task.emit(
                    "ai-analyze-error",
                    AiErrorEvent {
                        scan_id: scan_id_for_task,
                        message: e.to_string(),
                    },
                );
                return;
            }
            Err(e) => {
                tracing::error!("[ai] analyze task join failed: {e}");
                return;
            }
        };

        ai_state_for_task.store_suggestions(&suggestions);
        let _ = app_for_task.emit(
            SUGGESTIONS_EVENT,
            AiSuggestionsEvent {
                scan_id: scan_id_for_task,
                suggestions,
            },
        );
    });

    Ok(())
}

#[tauri::command]
pub fn cmd_ai_apply(
    suggestion_id: String,
    ai_state: State<'_, Arc<AiState>>,
) -> Result<String, String> {
    let suggestion = ai_state
        .get_suggestion(&suggestion_id)
        .ok_or_else(|| format!("no suggestion with id={suggestion_id}"))?;
    let real_path = ai_state
        .resolve_token(&suggestion.target_token)
        .ok_or_else(|| format!("token {} not in reverse table", suggestion.target_token))?;
    Ok(real_path)
}

#[tauri::command]
pub fn cmd_settings_get(
    path: String,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<Value, String> {
    Ok(settings.get(&path))
}

#[tauri::command]
pub fn cmd_settings_set(
    path: String,
    value: Value,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<(), String> {
    settings.set(&path, value).map_err(|e| e.to_string())
}
