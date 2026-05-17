use anyhow::Result;
use std::path::{Path, PathBuf};

use super::{directory_size, drive_letter_of, GameInfo, GameLibraryInfo, GamePlatform};

const XBOX_GAMES_HINT: &str =
    "新版 Game Pass 游戏，可直接 junction 到目标盘";
const WINDOWS_APPS_HINT: &str =
    "WindowsApps 受 TrustedInstaller 保护，请用「在系统设置中迁移」";

/// Default `XboxGames` directory on Windows.
#[cfg(windows)]
pub fn default_xbox_games_dir() -> Option<PathBuf> {
    let system_drive = std::env::var_os("SystemDrive")
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "C:".to_string());
    Some(PathBuf::from(format!("{system_drive}\\XboxGames")))
}

#[cfg(not(windows))]
pub fn default_xbox_games_dir() -> Option<PathBuf> {
    None
}

/// Default `WindowsApps` directory on Windows.
#[cfg(windows)]
pub fn default_windows_apps_dir() -> Option<PathBuf> {
    let program_files = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Program Files"));
    Some(program_files.join("WindowsApps"))
}

#[cfg(not(windows))]
pub fn default_windows_apps_dir() -> Option<PathBuf> {
    None
}

/// Detects Game Pass / Microsoft Store games installed in known locations.
pub fn detect() -> Result<Option<GameLibraryInfo>> {
    let xbox = default_xbox_games_dir();
    let windows_apps = default_windows_apps_dir();

    let info = detect_with_dirs(xbox.as_deref(), windows_apps.as_deref())?;
    if !info.installed {
        return Ok(Some(info));
    }
    Ok(Some(info))
}

/// Lower-level entry point used by tests with mocked install roots.
pub fn detect_with_dirs(
    xbox_games_dir: Option<&Path>,
    windows_apps_dir: Option<&Path>,
) -> Result<GameLibraryInfo> {
    let mut library_paths = Vec::new();
    let mut games = Vec::new();

    let mut xbox_present = false;
    if let Some(xbox) = xbox_games_dir {
        if xbox.exists() {
            xbox_present = true;
            library_paths.push(xbox.to_string_lossy().to_string());
            collect_xbox_games(xbox, &mut games);
        }
    }

    let mut store_present = false;
    if let Some(windows_apps) = windows_apps_dir {
        if windows_apps.exists() {
            store_present = true;
            library_paths.push(windows_apps.to_string_lossy().to_string());
            collect_windows_store_games(windows_apps, &mut games);
        }
    }

    games.sort_by(|a, b| b.install_size.cmp(&a.install_size).then(a.name.cmp(&b.name)));

    Ok(GameLibraryInfo {
        platform: GamePlatform::GamePass,
        library_paths,
        games,
        installed: xbox_present || store_present,
    })
}

fn collect_xbox_games(root: &Path, games: &mut Vec<GameInfo>) {
    for entry in std::fs::read_dir(root).into_iter().flatten() {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let install_path = path.to_string_lossy().to_string();
        let install_size = directory_size(&path);
        let drive_letter = drive_letter_of(&install_path);
        let migration_hint = if drive_letter.eq_ignore_ascii_case("C:") {
            XBOX_GAMES_HINT.to_string()
        } else {
            format!("游戏当前在 {drive_letter}，无需迁移")
        };
        games.push(GameInfo {
            platform: GamePlatform::GamePass,
            app_id: name.clone(),
            name,
            install_path,
            install_size,
            drive_letter,
            last_played: None,
            can_migrate: true,
            migration_hint,
        });
    }
}

fn collect_windows_store_games(root: &Path, games: &mut Vec<GameInfo>) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => {
            tracing::info!(
                "[games-gamepass] WindowsApps not readable, skipping enumeration"
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !looks_like_game_package(&name) {
            continue;
        }
        let install_path = entry.path().to_string_lossy().to_string();
        let drive_letter = drive_letter_of(&install_path);
        games.push(GameInfo {
            platform: GamePlatform::MicrosoftStore,
            app_id: name.clone(),
            name: friendly_package_name(&name),
            install_path,
            install_size: 0,
            drive_letter,
            last_played: None,
            can_migrate: false,
            migration_hint: WINDOWS_APPS_HINT.to_string(),
        });
    }
}

fn looks_like_game_package(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains(".game.")
        || lower.contains("xboxgame")
        || lower.contains("microsoft.")
            && (lower.contains("game") || lower.contains("studio"))
}

fn friendly_package_name(raw: &str) -> String {
    raw.split('_').next().unwrap_or(raw).replace('.', " ")
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
    fn xbox_games_are_migratable() {
        let root = fixture_dir("gamepass-xbox");
        let xbox = root.join("XboxGames");
        let game = xbox.join("Sea of Thieves");
        let content = game.join("Content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join("data.bin"), vec![0u8; 1024]).unwrap();

        let info = detect_with_dirs(Some(&xbox), None).unwrap();
        assert_eq!(info.platform, GamePlatform::GamePass);
        assert_eq!(info.games.len(), 1);
        assert!(info.games[0].can_migrate);
        assert_eq!(info.games[0].name, "Sea of Thieves");
        assert!(info.games[0].install_size >= 1024);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_store_games_are_marked_blocked() {
        let root = fixture_dir("gamepass-store");
        let store = root.join("WindowsApps");
        std::fs::create_dir_all(
            store.join("Microsoft.GamingApp_8wekyb3d8bbwe"),
        )
        .unwrap();
        std::fs::create_dir_all(
            store.join("Microsoft.Forza.Game.MotorsportRetail_8wekyb3d8bbwe"),
        )
        .unwrap();
        std::fs::create_dir_all(store.join("Some.Random.App_pub")).unwrap();

        let info = detect_with_dirs(None, Some(&store)).unwrap();
        assert!(info.installed);
        assert!(info.games.iter().any(|g| !g.can_migrate));
        let migratables = info.games.iter().filter(|g| g.can_migrate).count();
        assert_eq!(migratables, 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_dirs_yield_empty_uninstalled_info() {
        let info = detect_with_dirs(None, None).unwrap();
        assert!(!info.installed);
        assert!(info.games.is_empty());
    }
}
