use std::path::Path;

#[cfg(windows)]
pub fn normalized_path_key(path: &Path) -> String {
    normalize_windows_path_text(&path.to_string_lossy())
}

#[cfg(not(windows))]
pub fn normalized_path_key(path: &Path) -> String {
    normalize_non_windows_path_text(&path.to_string_lossy())
}

#[cfg(windows)]
pub fn normalized_path_key_str(path: &str) -> String {
    normalize_windows_path_text(path)
}

#[cfg(not(windows))]
pub fn normalized_path_key_str(path: &str) -> String {
    normalize_non_windows_path_text(path)
}

pub fn normalized_path_starts_with(candidate_key: &str, prefix_key: &str) -> bool {
    if prefix_key.is_empty() {
        return false;
    }
    if candidate_key == prefix_key {
        return true;
    }
    if !candidate_key.starts_with(prefix_key) {
        return false;
    }
    if prefix_key
        .as_bytes()
        .last()
        .is_some_and(|byte| is_path_separator(*byte))
    {
        return true;
    }
    candidate_key
        .as_bytes()
        .get(prefix_key.len())
        .is_some_and(|byte| is_path_separator(*byte))
}

#[cfg(windows)]
fn normalize_windows_path_text(path: &str) -> String {
    let mut text = path.replace('/', "\\").to_ascii_lowercase();
    while text.ends_with('\\') && !is_windows_root(&text) {
        text.pop();
    }
    text
}

#[cfg(not(windows))]
fn normalize_non_windows_path_text(path: &str) -> String {
    let mut text = path.to_string();
    while text.ends_with('/') && text.len() > 1 {
        text.pop();
    }
    text
}

#[cfg(windows)]
fn is_windows_root(path: &str) -> bool {
    if path.len() == 3 {
        let bytes = path.as_bytes();
        return bytes[1] == b':' && bytes[2] == b'\\';
    }
    if path.starts_with("\\\\") {
        return path.matches('\\').count() <= 3;
    }
    false
}

fn is_path_separator(byte: u8) -> bool {
    byte == b'\\' || byte == b'/'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_prefix_respects_path_boundaries() {
        let prefix = normalized_path_key_str(r"C:\foo");
        let child = normalized_path_key_str(r"C:\foo\bar.txt");
        let sibling = normalized_path_key_str(r"C:\foobar\bar.txt");

        assert!(normalized_path_starts_with(&child, &prefix));
        assert!(!normalized_path_starts_with(&sibling, &prefix));
    }

    #[test]
    fn normalized_prefix_matches_equal_paths() {
        let path = normalized_path_key_str(r"C:\Foo\");
        let same = normalized_path_key_str(r"c:/foo");

        assert!(normalized_path_starts_with(&path, &same));
    }

    #[test]
    fn normalized_prefix_handles_drive_root() {
        let root = normalized_path_key_str(r"C:\");
        let child = normalized_path_key_str(r"C:\foo\bar.txt");

        assert!(normalized_path_starts_with(&child, &root));
    }
}
