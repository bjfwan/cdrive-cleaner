use anyhow::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::{directory_size, drive_letter_of, GameInfo, GameLibraryInfo, GamePlatform};

#[derive(Debug, Deserialize)]
struct EpicManifest {
    #[serde(rename = "InstallLocation")]
    install_location: Option<String>,
    #[serde(rename = "DisplayName")]
    display_name: Option<String>,
    #[serde(rename = "AppName")]
    app_name: Option<String>,
    #[serde(rename = "InstallSize")]
    install_size: Option<u64>,
    #[serde(rename = "MainGameAppName")]
    main_game_app_name: Option<String>,
    #[serde(rename = "bIsApplication")]
    is_application: Option<bool>,
    #[serde(rename = "bIsExecutable")]
    is_executable: Option<bool>,
}

/// Default Epic manifest directory on Windows.
#[cfg(windows)]
pub fn default_manifests_dir() -> Option<PathBuf> {
    let program_data = std::env::var_os("ProgramData").map(PathBuf::from)?;
    Some(
        program_data
            .join("Epic")
            .join("EpicGamesLauncher")
            .join("Data")
            .join("Manifests"),
    )
}

#[cfg(not(windows))]
pub fn default_manifests_dir() -> Option<PathBuf> {
    None
}

/// Detects all Epic Games Launcher manifests on the current machine.
pub fn detect() -> Result<Option<GameLibraryInfo>> {
    let Some(manifests) = default_manifests_dir() else {
        return Ok(None);
    };
    if !manifests.exists() {
        return Ok(Some(GameLibraryInfo {
            platform: GamePlatform::Epic,
            library_paths: vec![],
            games: vec![],
            installed: false,
        }));
    }
    detect_with_dir(&manifests).map(Some)
}

/// Lower-level entry point used by tests with a mocked manifest directory.
pub fn detect_with_dir(manifests_dir: &Path) -> Result<GameLibraryInfo> {
    let mut games = Vec::new();
    let mut libraries: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(manifests_dir).into_iter().flatten() {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("item") {
            continue;
        }
        match parse_manifest(&path) {
            Ok(Some(game)) => {
                if let Some(parent) = Path::new(&game.install_path).parent() {
                    let parent_str = parent.to_string_lossy().to_string();
                    if !libraries
                        .iter()
                        .any(|p| p.eq_ignore_ascii_case(&parent_str))
                    {
                        libraries.push(parent_str);
                    }
                }
                games.push(game);
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!("[games-epic] failed to parse {}: {}", path.display(), err);
            }
        }
    }

    games.sort_by(|a, b| b.install_size.cmp(&a.install_size).then(a.name.cmp(&b.name)));

    Ok(GameLibraryInfo {
        platform: GamePlatform::Epic,
        library_paths: libraries,
        games,
        installed: true,
    })
}

fn parse_manifest(path: &Path) -> Result<Option<GameInfo>> {
    let raw = std::fs::read_to_string(path)?;
    let manifest: EpicManifest = serde_json::from_str(&raw)?;

    if manifest.is_application == Some(false) || manifest.is_executable == Some(false) {
        return Ok(None);
    }
    if let Some(parent) = manifest.main_game_app_name.as_deref() {
        if let Some(app) = manifest.app_name.as_deref() {
            if !parent.eq_ignore_ascii_case(app) {
                return Ok(None);
            }
        }
    }

    let install_location = match manifest.install_location {
        Some(p) if !p.is_empty() => p,
        _ => return Ok(None),
    };
    let app_id = manifest.app_name.unwrap_or_default();
    let name = manifest
        .display_name
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| app_id.clone());
    let install_path_buf = PathBuf::from(&install_location);
    let install_size = manifest
        .install_size
        .filter(|&s| s > 0)
        .unwrap_or_else(|| directory_size(&install_path_buf));

    let drive_letter = drive_letter_of(&install_location);
    let migration_hint = if drive_letter.eq_ignore_ascii_case("C:") {
        "可直接 junction 到目标盘，启动器会做一次完整性校验".to_string()
    } else {
        format!("游戏当前在 {drive_letter}，无需迁移")
    };

    Ok(Some(GameInfo {
        platform: GamePlatform::Epic,
        app_id,
        name,
        install_path: install_location,
        install_size,
        drive_letter,
        last_played: None,
        can_migrate: true,
        migration_hint,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(name: &str) -> PathBuf {
        let unique = format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn parses_item_manifests() {
        let dir = fixture_dir("epic-detect");
        let game_path = dir.join("Game1");
        std::fs::create_dir_all(&game_path).unwrap();

        let json = serde_json::json!({
            "InstallLocation": game_path.to_string_lossy(),
            "DisplayName": "My Adventure",
            "AppName": "0a1b2c3d4e",
            "InstallSize": 12345678u64,
            "bIsApplication": true,
            "bIsExecutable": true,
        });
        std::fs::write(dir.join("0a1b2c3d4e.item"), json.to_string()).unwrap();

        std::fs::write(
            dir.join("dlc.item"),
            serde_json::json!({
                "InstallLocation": dir.join("Game1").to_string_lossy(),
                "DisplayName": "DLC",
                "AppName": "child",
                "MainGameAppName": "0a1b2c3d4e",
                "InstallSize": 1024u64,
                "bIsApplication": true,
            })
            .to_string(),
        )
        .unwrap();

        let info = detect_with_dir(&dir).unwrap();
        assert_eq!(info.platform, GamePlatform::Epic);
        assert_eq!(info.games.len(), 1);
        assert_eq!(info.games[0].name, "My Adventure");
        assert_eq!(info.games[0].install_size, 12345678);
        assert!(info.games[0].can_migrate);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_invalid_files() {
        let dir = fixture_dir("epic-bad");
        std::fs::write(dir.join("broken.item"), "not json").unwrap();
        std::fs::write(
            dir.join("noloc.item"),
            serde_json::json!({"DisplayName": "Empty", "AppName": "x"}).to_string(),
        )
        .unwrap();

        let info = detect_with_dir(&dir).unwrap();
        assert!(info.games.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
