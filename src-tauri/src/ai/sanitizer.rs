use std::collections::HashMap;
use std::path::Path;

use crate::scanner::file_info::{DirectoryNode, FileInfo, ScanResult};

use super::types::{
    AiSettings, AiSnapshot, CategorySummary, DiskSummary, LargeItemSummary,
};

const MIN_DIR_BYTES: u64 = 100 * 1024 * 1024;
const MIN_FILE_BYTES: u64 = 256 * 1024 * 1024;

pub struct SanitizedSnapshot {
    pub snapshot: AiSnapshot,
    pub token_to_path: HashMap<String, String>,
}

pub fn build_snapshot(
    scan: &ScanResult,
    disk_total: u64,
    disk_free: u64,
    settings: &AiSettings,
) -> SanitizedSnapshot {
    let mut token_to_path: HashMap<String, String> = HashMap::new();
    let mut candidates: Vec<(String, String, u64, String)> = Vec::new();

    collect_dirs(&scan.directories, &mut candidates);

    for f in &scan.large_files {
        if f.size < MIN_FILE_BYTES {
            continue;
        }
        let kind = classify(&f.path, true);
        let label = sanitize_label(&f.name, &f.path, f.size);
        candidates.push((label, f.path.clone(), f.size, kind));
    }

    candidates.retain(|c| settings.categories.allows(&c.3));
    candidates.sort_by(|a, b| b.2.cmp(&a.2));

    let mut category_totals: HashMap<String, (u64, usize)> = HashMap::new();
    for c in &candidates {
        let entry = category_totals.entry(c.3.clone()).or_insert((0, 0));
        entry.0 += c.2;
        entry.1 += 1;
    }

    let cap = if settings.max_items == 0 { 20 } else { settings.max_items };
    candidates.truncate(cap);

    let mut large_items: Vec<LargeItemSummary> = Vec::with_capacity(candidates.len());
    for (label, real_path, size, kind) in candidates {
        let token = sha1_short(&real_path);
        token_to_path.insert(token.clone(), real_path);
        large_items.push(LargeItemSummary {
            label,
            token,
            size_mb: (size / (1024 * 1024)).max(1),
            kind,
        });
    }

    let categories: Vec<CategorySummary> = category_totals
        .into_iter()
        .map(|(kind, (total, count))| CategorySummary {
            kind,
            total_mb: total / (1024 * 1024),
            item_count: count,
        })
        .collect();

    let drive = drive_letter(&scan.root_path);
    let disks = vec![DiskSummary {
        drive,
        total_gb: disk_total / (1024 * 1024 * 1024),
        free_gb: disk_free / (1024 * 1024 * 1024),
    }];

    SanitizedSnapshot {
        snapshot: AiSnapshot {
            disks,
            large_items,
            categories,
        },
        token_to_path,
    }
}

fn collect_dirs(nodes: &[DirectoryNode], out: &mut Vec<(String, String, u64, String)>) {
    for node in nodes {
        if node.size >= MIN_DIR_BYTES {
            let kind = classify(&node.path, false);
            let label = sanitize_label(&node.name, &node.path, node.size);
            out.push((label, node.path.clone(), node.size, kind));
        }
        if !node.children.is_empty() {
            collect_dirs(&node.children, out);
        }
    }
}

fn classify(path: &str, is_file: bool) -> String {
    let lower = path.to_ascii_lowercase();
    let backslashed = lower.replace('/', "\\");

    // System files — never delete, may be resized/moved
    if is_file {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name == "hiberfil.sys" || name == "pagefile.sys" || name == "swapfile.sys" {
            return "system_files".into();
        }
        if name == "ntuser.dat" || name.ends_with(".regtrans-ms") || name.ends_with(".blf") {
            return "system_files".into();
        }
    }

    // Temp / recycle
    if backslashed.contains("\\appdata\\local\\temp")
        || backslashed.contains("\\temp\\")
        || backslashed.ends_with("\\temp")
        || backslashed.contains("\\$recycle.bin")
        || backslashed.contains("\\windows\\softwaredistribution")
        || backslashed.contains("\\windows\\temp")
    {
        return "temp_files".into();
    }

    // Dev / build artifacts
    if backslashed.contains("\\node_modules")
        || backslashed.contains("\\.gradle")
        || backslashed.contains("\\.m2")
        || backslashed.contains("\\target\\debug")
        || backslashed.contains("\\target\\release")
        || backslashed.contains("\\.cargo")
        || backslashed.contains("\\go\\pkg")
        || backslashed.contains("\\.pnpm-store")
        || backslashed.contains("\\.tox")
        || backslashed.contains("\\__pycache__")
        || backslashed.contains("\\.venv")
        || backslashed.contains("\\venv")
        || backslashed.contains("\\dist")
        || backslashed.contains("\\build")
    {
        return "dev_tools".into();
    }

    // Cache directories
    if backslashed.contains("\\cache")
        || backslashed.contains("\\caches")
        || backslashed.contains("\\appdata\\local\\packages")
        || backslashed.contains("\\appdata\\roaming")
        || backslashed.contains("\\.cache")
    {
        return "app_cache".into();
    }

    // Model / ML files — large, movable
    if is_file {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.ends_with(".bin") && name.contains("model")
            || name.ends_with(".safetensors")
            || name.ends_with(".pt")
            || name.ends_with(".pth")
            || name.ends_with(".onnx")
            || name.ends_with(".gguf")
            || name.ends_with(".ckpt")
            || name.contains("pytorch_model")
            || name.contains("tf_model")
        {
            return "model_files".into();
        }
    }
    if backslashed.contains("\\models")
        || backslashed.contains("\\checkpoints")
        || backslashed.contains("\\huggingface")
        || backslashed.contains("\\ollama")
        || backslashed.contains("\\.cache\\huggingface")
        || backslashed.contains("\\.cache\\lm-studio")
    {
        return "model_files".into();
    }

    // Game installations
    if backslashed.contains("\\steamapps\\common")
        || backslashed.contains("\\steamapps\\downloading")
        || backslashed.contains("\\steamapps\\workshop")
        || backslashed.contains("\\origin games")
        || backslashed.contains("\\epic games")
        || backslashed.contains("\\gog games")
        || backslashed.contains("\\ubisoft")
        || backslashed.contains("\\blizzard")
    {
        return "game_files".into();
    }

    // VMs and disk images
    if is_file {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.ends_with(".vhd") || name.ends_with(".vhdx")
            || name.ends_with(".vmdk") || name.ends_with(".ova")
            || name.ends_with(".iso") || name.ends_with(".img")
            || name.ends_with(".dmg")
        {
            return "disk_images".into();
        }
    }

    // Installers / archives
    if is_file {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.ends_with(".msi") || name.ends_with(".exe")
            && !name.contains("setup")
            && backslashed.contains("\\downloads")
        {
            return "installer_files".into();
        }
    }
    if backslashed.contains("\\downloads") {
        return "downloads".into();
    }

    // Large media
    if is_file {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.ends_with(".mp4") || name.ends_with(".mkv") || name.ends_with(".avi")
            || name.ends_with(".mov") || name.ends_with(".wmv") || name.ends_with(".flv")
            || name.ends_with(".mp3") || name.ends_with(".flac") || name.ends_with(".wav")
            || name.ends_with(".psd") || name.ends_with(".ai") || name.ends_with(".aep")
            || name.ends_with(".prproj") || name.ends_with(".fcpxml")
        {
            return "media_files".into();
        }
    }
    if backslashed.contains("\\videos")
        || backslashed.contains("\\video")
        || backslashed.contains("\\movies")
        || backslashed.contains("\\music")
        || backslashed.contains("\\pictures")
        || backslashed.contains("\\photos")
    {
        return "media_files".into();
    }

    // Logs
    if is_file {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.ends_with(".log") || name.ends_with(".etl") {
            return "log_files".into();
        }
    }
    if backslashed.contains("\\logs") || backslashed.contains("\\log") {
        return "log_files".into();
    }

    if is_file {
        return "large_files".into();
    }
    "large_dirs".into()
}

fn sanitize_label(name: &str, path: &str, size_bytes: u64) -> String {
    let safe_name = if name.trim().is_empty() {
        Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "(unnamed)".into())
    } else {
        name.to_string()
    };
    format!("{} ({})", safe_name, format_size(size_bytes))
}

fn format_size(bytes: u64) -> String {
    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gb >= 1.0 {
        return format!("{:.1} GB", gb);
    }
    let mb = bytes as f64 / (1024.0 * 1024.0);
    format!("{:.0} MB", mb)
}

fn drive_letter(root_path: &str) -> String {
    let trimmed = root_path.trim();
    if let Some(first) = trimmed.chars().next() {
        if first.is_ascii_alphabetic() {
            return format!("{}:", first.to_ascii_uppercase());
        }
    }
    "?".into()
}

fn sha1_short(input: &str) -> String {
    let digest = sha1_digest(input.as_bytes());
    let mut out = String::with_capacity(8);
    for b in digest.iter().take(4) {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn sha1_digest(message: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (message.len() as u64) * 8;
    let mut padded: Vec<u8> = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = if i < 20 {
                ((b & c) | ((!b) & d), 0x5A827999u32)
            } else if i < 40 {
                (b ^ c ^ d, 0x6ED9EBA1u32)
            } else if i < 60 {
                ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32)
            } else {
                (b ^ c ^ d, 0xCA62C1D6u32)
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

#[allow(dead_code)]
pub fn collect_files(_files: &[FileInfo]) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_short_is_stable_and_hex() {
        let a = sha1_short("C:\\Users\\bob\\Downloads");
        let b = sha1_short("C:\\Users\\bob\\Downloads");
        assert_eq!(a, b);
        assert_eq!(a.len(), 8);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sha1_short_differs_per_path() {
        let a = sha1_short("C:\\path\\a");
        let b = sha1_short("C:\\path\\b");
        assert_ne!(a, b);
    }

    #[test]
    fn snapshot_strips_paths_and_usernames() {
        let scan = ScanResult {
            root_path: "C:\\".into(),
            total_size: 0,
            system_reserved_bytes: 0,
            total_files: 0,
            total_dirs: 0,
            scan_duration_ms: 0,
            directories: vec![DirectoryNode {
                path: "C:\\Users\\alice\\AppData\\Local\\Temp\\big".into(),
                name: "big".into(),
                size: 500 * 1024 * 1024,
                file_count: 10,
                dir_count: 0,
                children: vec![],
                has_children: false,
                is_symlink: false,
                link_target: None,
                safety: None,
                modified_time: None,
                file_id: None,
            }],
            large_files: vec![],
            inaccessible_count: 0,
            scan_backend: None,
            root_file_id: None,
            usn_journal_id: None,
            usn_next_usn: None,
            cache_schema_version: 0,
            env_fingerprint: Default::default(),
            scan_completed: true,
        };
        let settings = AiSettings::default();
        let res = build_snapshot(&scan, 100, 50, &settings);
        let json = serde_json::to_string(&res.snapshot).unwrap();
        assert!(!json.contains("alice"));
        assert!(!json.contains("C:\\Users"));
        assert!(!json.contains("AppData"));
    }
}
