pub mod epic;
pub mod gamepass;
pub mod steam;
pub mod vdf;

use serde::{Deserialize, Serialize};

/// Identifies which launcher owns a game library.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GamePlatform {
    Steam,
    Epic,
    GamePass,
    MicrosoftStore,
}

impl GamePlatform {
    /// Returns the human-readable platform label used in the database.
    pub fn label(&self) -> &'static str {
        match self {
            GamePlatform::Steam => "Steam",
            GamePlatform::Epic => "Epic",
            GamePlatform::GamePass => "GamePass",
            GamePlatform::MicrosoftStore => "MicrosoftStore",
        }
    }
}

/// Information about a single installed game.
#[derive(Debug, Clone, Serialize)]
pub struct GameInfo {
    pub platform: GamePlatform,
    pub app_id: String,
    pub name: String,
    pub install_path: String,
    pub install_size: u64,
    pub drive_letter: String,
    pub last_played: Option<String>,
    pub can_migrate: bool,
    pub migration_hint: String,
}

/// Information about a launcher and its games.
#[derive(Debug, Clone, Serialize)]
pub struct GameLibraryInfo {
    pub platform: GamePlatform,
    pub library_paths: Vec<String>,
    pub games: Vec<GameInfo>,
    pub installed: bool,
}

/// Extracts an upper-case drive letter (e.g. `C:`) from a Windows path.
pub fn drive_letter_of(path: &str) -> String {
    let mut chars = path.chars();
    let first = chars.next().unwrap_or('?');
    let second = chars.next().unwrap_or('?');
    if first.is_ascii_alphabetic() && second == ':' {
        let mut letter = String::new();
        letter.push(first.to_ascii_uppercase());
        letter.push(':');
        letter
    } else {
        String::new()
    }
}

/// Sums a directory tree's file sizes. Returns 0 if the path cannot be walked.
pub fn directory_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    for entry in jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
    {
        if let Ok(entry) = entry {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total = total.saturating_add(meta.len());
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_letter_extracts_uppercase() {
        assert_eq!(drive_letter_of("c:\\Games\\My Game"), "C:");
        assert_eq!(drive_letter_of("D:\\Foo"), "D:");
        assert_eq!(drive_letter_of("\\\\server\\share"), "");
        assert_eq!(drive_letter_of(""), "");
    }
}
