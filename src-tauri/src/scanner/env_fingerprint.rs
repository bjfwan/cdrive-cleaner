use serde::{Deserialize, Serialize};
use std::path::Path;
pub const CACHE_SCHEMA_VERSION: u32 = 1;

/// 清理规则版本号，预留给清理规则升级。
pub const RULE_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EnvFingerprint {
    #[serde(default)]
    pub volume_serial: Option<u64>,
    #[serde(default)]
    pub file_system: String,
    #[serde(default)]
    pub is_elevated: bool,
    #[serde(default)]
    pub user_sid: Option<String>,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub rule_version: u32,
}

impl EnvFingerprint {
    /// 计算当前进程 + 目标路径所在卷的环境指纹。
    pub fn current(path: &Path) -> EnvFingerprint {
        let (volume_serial, file_system) = current_volume_info(path);
        EnvFingerprint {
            volume_serial,
            file_system,
            is_elevated: crate::commands::is_elevated(),
            user_sid: current_user_sid(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            rule_version: RULE_VERSION,
        }
    }
}

#[cfg(windows)]
fn current_volume_info(path: &Path) -> (Option<u64>, String) {
    let file_system = crate::winfs::query_volume_details(path)
        .map(|details| details.file_system)
        .unwrap_or_default();
    let serial = query_volume_serial_number(path);
    (serial, file_system)
}

#[cfg(not(windows))]
fn current_volume_info(_path: &Path) -> (Option<u64>, String) {
    (None, String::new())
}

#[cfg(windows)]
fn query_volume_serial_number(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;

    let path_str = path.to_string_lossy();
    let bytes = path_str.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' {
        return None;
    }
    let root = format!("{}:\\", bytes[0] as char);
    let wide: Vec<u16> = std::ffi::OsStr::new(&root)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut serial: u32 = 0;
    let ok = unsafe {
        GetVolumeInformationW(
            PCWSTR(wide.as_ptr()),
            None,
            None,
            Some(&mut serial),
            None,
            None,
        )
    };
    if ok.is_ok() {
        Some(serial as u64)
    } else {
        None
    }
}

#[cfg(windows)]
fn current_user_sid() -> Option<String> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return None;
        }
        let _guard = OwnedHandle(token);

        let mut needed: u32 = 0;
        // 第一次调用拿 buffer 大小，预期会失败但会写入 needed。
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
        if needed == 0 {
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
        .is_err()
        {
            return None;
        }
        let token_user = &*(buf.as_ptr() as *const TOKEN_USER);
        let psid = token_user.User.Sid.0;
        if psid.is_null() {
            return None;
        }
        sid_to_string(psid as *const u8)
    }
}

#[cfg(windows)]
unsafe fn sid_to_string(sid: *const u8) -> Option<String> {
    if sid.is_null() {
        return None;
    }
    // SID 二进制布局：1B revision + 1B sub_authority_count + 6B identifier_authority(big-endian)
    // + N*4 bytes sub-authorities(little-endian)
    let revision = *sid;
    let count = *sid.add(1) as usize;
    if count > 15 {
        // Windows 限制 SID 子授权数 ≤ 15；越界视为无效。
        return None;
    }
    let mut authority: u64 = 0;
    for i in 0..6 {
        authority = (authority << 8) | (*sid.add(2 + i)) as u64;
    }
    let mut text = format!("S-{revision}-{authority}");
    for i in 0..count {
        let off = 8 + i * 4;
        let sub = u32::from_le_bytes([
            *sid.add(off),
            *sid.add(off + 1),
            *sid.add(off + 2),
            *sid.add(off + 3),
        ]);
        text.push_str(&format!("-{sub}"));
    }
    Some(text)
}

#[cfg(not(windows))]
fn current_user_sid() -> Option<String> {
    None
}
