use anyhow::Result;
use std::path::{Path, PathBuf};

use super::vdf::{self, VdfValue};
use super::{directory_size, drive_letter_of, GameInfo, GameLibraryInfo, GamePlatform};

/// Returns the Steam install path from `HKCU\Software\Valve\Steam\SteamPath`.
#[cfg(windows)]
pub fn read_steam_path() -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey("Software\\Valve\\Steam").ok()?;
    let raw: String = key.get_value("SteamPath").ok()?;
    let normalised = raw.replace('/', "\\");
    Some(PathBuf::from(normalised))
}

#[cfg(not(windows))]
pub fn read_steam_path() -> Option<PathBuf> {
    None
}

/// Detects all Steam libraries and the installed games on the current machine.
pub fn detect() -> Result<Option<GameLibraryInfo>> {
    let Some(steam_path) = read_steam_path() else {
        return Ok(None);
    };

    if !steam_path.exists() {
        return Ok(Some(GameLibraryInfo {
            platform: GamePlatform::Steam,
            library_paths: vec![],
            games: vec![],
            installed: false,
        }));
    }

    detect_with_root(&steam_path).map(Some)
}

/// Lower-level entry point used by tests with a mocked Steam root.
pub fn detect_with_root(steam_path: &Path) -> Result<GameLibraryInfo> {
    let library_paths = read_library_folders(steam_path)?;

    let mut games = Vec::new();
    for library in &library_paths {
        let common = Path::new(library).join("steamapps").join("common");
        let library_path = Path::new(library).join("steamapps");
        if !library_path.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&library_path).into_iter().flatten() {
            let Ok(entry) = entry else { continue };
            let name = entry.file_name();
            let Some(file_name) = name.to_str() else {
                continue;
            };
            if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
                continue;
            }
            let acf_path = entry.path();
            match parse_appmanifest(&acf_path, &common) {
                Ok(Some(game)) => games.push(game),
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(
                        "[games-steam] failed to parse {}: {}",
                        acf_path.display(),
                        err
                    );
                }
            }
        }
    }

    games.sort_by(|a, b| b.install_size.cmp(&a.install_size).then(a.name.cmp(&b.name)));

    Ok(GameLibraryInfo {
        platform: GamePlatform::Steam,
        library_paths,
        games,
        installed: true,
    })
}

fn read_library_folders(steam_path: &Path) -> Result<Vec<String>> {
    let vdf_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    let mut libraries = vec![steam_path.to_string_lossy().to_string()];
    if !vdf_path.exists() {
        return Ok(libraries);
    }

    let raw = std::fs::read_to_string(&vdf_path)?;
    let parsed = vdf::parse(&raw)?;
    let Some(folders) = parsed.get_ci("libraryfolders") else {
        return Ok(libraries);
    };
    let Some(map) = folders.as_object() else {
        return Ok(libraries);
    };

    for (key, value) in map.iter() {
        if !key.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let path_value = match value {
            VdfValue::Object(_) => value.get_ci("path").and_then(|v| v.as_str()),
            VdfValue::String(s) => Some(s.as_str()),
        };
        let Some(path) = path_value else {
            continue;
        };
        let owned = path.to_string();
        if libraries.iter().any(|p| p.eq_ignore_ascii_case(&owned)) {
            continue;
        }
        libraries.push(owned);
    }

    Ok(libraries)
}

fn parse_appmanifest(path: &Path, common_dir: &Path) -> Result<Option<GameInfo>> {
    let raw = std::fs::read_to_string(path)?;
    let parsed = vdf::parse(&raw)?;
    let Some(state) = parsed.get_ci("AppState") else {
        return Ok(None);
    };

    let app_id = state
        .get_ci("appid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name = state
        .get_ci("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let install_dir = state
        .get_ci("installdir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if install_dir.is_empty() {
        return Ok(None);
    }
    let size_on_disk = state
        .get_ci("SizeOnDisk")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok());
    let last_played = state
        .get_ci("LastPlayed")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|&secs| secs > 0)
        .map(|secs| {
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default()
        });

    let install_path = common_dir.join(&install_dir);
    let install_path_str = install_path.to_string_lossy().to_string();
    let install_size = size_on_disk.unwrap_or_else(|| directory_size(&install_path));
    let drive_letter = drive_letter_of(&install_path_str);

    let migration_hint = if drive_letter.eq_ignore_ascii_case("C:") {
        "可直接 junction 到目标盘，Steam 不在乎物理路径".to_string()
    } else {
        format!("游戏当前在 {drive_letter}，无需迁移")
    };

    Ok(Some(GameInfo {
        platform: GamePlatform::Steam,
        app_id,
        name,
        install_path: install_path_str,
        install_size,
        drive_letter,
        last_played,
        can_migrate: true,
        migration_hint,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn fixture_root(name: &str) -> PathBuf {
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
    fn parses_library_folders_and_appmanifests() {
        let root = fixture_root("steam-detect");
        let steam_root = root.join("Steam");
        let lib_other = root.join("OtherLibrary");

        let library_vdf = format!(
            r#"
"libraryfolders"
{{
    "0"
    {{
        "path"        "{steam}"
        "label"       ""
        "apps"
        {{
        }}
    }}
    "1"
    {{
        "path"        "{lib}"
        "apps"
        {{
        }}
    }}
}}
"#,
            steam = steam_root.to_string_lossy().replace('\\', "\\\\"),
            lib = lib_other.to_string_lossy().replace('\\', "\\\\"),
        );

        write(
            &steam_root.join("steamapps").join("libraryfolders.vdf"),
            &library_vdf,
        );

        write(
            &steam_root
                .join("steamapps")
                .join("appmanifest_440.acf"),
            r#"
"AppState"
{
    "appid"       "440"
    "name"        "Team Fortress 2"
    "installdir"  "Team Fortress 2"
    "SizeOnDisk"  "26843545600"
    "LastPlayed"  "1700000000"
}
"#,
        );

        std::fs::create_dir_all(steam_root.join("steamapps").join("common").join("Team Fortress 2"))
            .unwrap();

        write(
            &lib_other.join("steamapps").join("appmanifest_292030.acf"),
            r#"
"AppState"
{
    "appid"       "292030"
    "name"        "The Witcher 3: Wild Hunt"
    "installdir"  "The Witcher 3"
    "SizeOnDisk"  "53687091200"
}
"#,
        );
        std::fs::create_dir_all(
            lib_other
                .join("steamapps")
                .join("common")
                .join("The Witcher 3"),
        )
        .unwrap();

        let info = detect_with_root(&steam_root).unwrap();
        assert_eq!(info.platform, GamePlatform::Steam);
        assert_eq!(info.library_paths.len(), 2);
        assert_eq!(info.games.len(), 2);
        let names: Vec<_> = info.games.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"Team Fortress 2"));
        assert!(names.contains(&"The Witcher 3: Wild Hunt"));
        for game in &info.games {
            assert!(game.install_path.contains("common"));
            assert!(game.can_migrate);
            assert!(!game.app_id.is_empty());
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn handles_missing_library_vdf() {
        let root = fixture_root("steam-no-vdf");
        let info = detect_with_root(&root).unwrap();
        assert_eq!(info.library_paths.len(), 1);
        assert!(info.games.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
