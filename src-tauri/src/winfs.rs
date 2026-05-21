use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct NativeDirEntry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified_time: Option<u64>,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_readonly: bool,
    pub file_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct UsnJournalCheckpoint {
    pub journal_id: u64,
    pub next_usn: i64,
}

#[derive(Debug, Clone)]
pub struct UsnChangeSet {
    pub recursive_dirs: HashSet<String>,
    pub direct_file_dirs: HashSet<String>,
    pub root_files_changed: bool,
    /// USN 路径上识别到的「老 path → 新 path」改名事件。增量合并阶段可用来
    /// 把"删旧 + 建新"识别成 rename，复用旧子树。识别失败时为空。
    pub renames: Vec<UsnRenameEvent>,
}

#[derive(Debug, Clone)]
pub struct UsnRenameEvent {
    /// USN 给的 FileReferenceNumber，便于诊断；当前合并阶段只按 path 字符串匹配。
    #[allow(dead_code)]
    pub file_id: u64,
    pub old_path: String,
    pub new_path: String,
    pub is_dir: bool,
}

impl UsnChangeSet {
    pub fn all_changed_dirs(&self) -> HashSet<String> {
        let mut all = self.recursive_dirs.clone();
        all.extend(self.direct_file_dirs.iter().cloned());
        all
    }
}

/// `collect_usn_changed_dirs` / `read_usn_journal_with_retry` 的结果。
///
/// - `Records`：拿到了一段完整的 USN 增量，里面已经按"递归重扫 / 直接文件重扫
///   / 根文件变化 / rename"分好类。
/// - `JournalReset`：FSCTL_QUERY_USN_JOURNAL 返回的 journal_id 跟 cache 里
///   存的不一样，比如卷的 journal 被重建过；此时调用方应该走"全量重建"路径，
///   不应再退化到 mtime 全量递归。
/// - `StartUsnTooOld`：cache 的 next_usn 比卷的 first_usn 还小，说明记录已经
///   被回卷丢了；处理方式同 `JournalReset`。
/// - `HardError`：FSCTL 调用本身失败（卷句柄打不开、权限不够等），调用方
///   可以视情况退化到 mtime。
pub enum UsnReadOutcome {
    Records(UsnChangeSet),
    JournalReset {
        new_journal_id: u64,
        first_usn: i64,
    },
    StartUsnTooOld {
        first_usn: i64,
    },
    HardError(std::io::Error),
}

pub fn resolve_link_target(path: &Path) -> Option<String> {
    let resolved = if let Ok(target) = std::fs::read_link(path) {
        if target.is_absolute() {
            target
        } else if let Some(parent) = path.parent() {
            parent.join(&target)
        } else {
            target
        }
    } else {
        let canonical = std::fs::canonicalize(path).ok()?;
        let original = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(path)
        };

        if canonical == original {
            return None;
        }

        canonical
    };

    Some(resolved.to_string_lossy().to_string())
}

pub fn paths_refer_to_same_file(a: &Path, b: &Path) -> Option<bool> {
    #[cfg(windows)]
    {
        let volume_a = query_volume_details(a)?.device_path;
        let volume_b = query_volume_details(b)?.device_path;
        if !volume_a.eq_ignore_ascii_case(&volume_b) {
            return Some(false);
        }
        Some(get_path_file_id(a)? == get_path_file_id(b)?)
    }

    #[cfg(not(windows))]
    {
        let canonical_a = std::fs::canonicalize(a).ok()?;
        let canonical_b = std::fs::canonicalize(b).ok()?;
        Some(canonical_a == canonical_b)
    }
}

#[derive(Debug, Clone)]
pub struct VolumeDetails {
    pub volume_root: PathBuf,
    pub device_path: String,
    pub file_system: String,
}

#[derive(Debug, Clone)]
pub struct MftEntry {
    pub file_id: u64,
    pub parent_file_id: u64,
    pub name: String,
    pub is_dir: bool,
    pub is_reparse_point: bool,
    pub is_readonly: bool,
}

#[derive(Debug, Clone)]
pub struct FileIdMetadata {
    pub size: u64,
    pub modified_time: Option<u64>,
    pub is_readonly: bool,
}

#[cfg(not(windows))]
use std::collections::{HashMap, HashSet};
#[cfg(not(windows))]
pub fn enumerate_directory(
    path: &Path,
    include_dir_file_ids: bool,
) -> std::io::Result<Vec<NativeDirEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = std::fs::symlink_metadata(&entry_path)?;
        let is_symlink = metadata.file_type().is_symlink();
        let is_dir = metadata.is_dir();
        let modified_time = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());

        entries.push(NativeDirEntry {
            path: entry_path,
            name: entry.file_name().to_string_lossy().to_string(),
            size: metadata.len(),
            modified_time,
            is_dir,
            is_symlink,
            is_readonly: metadata.permissions().readonly(),
            file_id: include_dir_file_ids && is_dir && !is_symlink.then_some(0).filter(|_| false),
        });
    }
    Ok(entries)
}

#[cfg(not(windows))]
pub fn get_path_file_id(_path: &Path) -> Option<u64> {
    None
}

#[cfg(not(windows))]
pub fn query_usn_checkpoint(_path: &Path) -> Option<UsnJournalCheckpoint> {
    None
}

#[cfg(not(windows))]
pub fn collect_usn_changed_dirs(
    _root_path: &Path,
    _checkpoint: UsnJournalCheckpoint,
    _root_file_id: Option<u64>,
    _frn_to_path: &HashMap<u64, String>,
) -> std::io::Result<Option<UsnChangeSet>> {
    Ok(None)
}

#[cfg(not(windows))]
pub fn read_usn_journal_with_retry(
    _root_path: &Path,
    _checkpoint: UsnJournalCheckpoint,
    _root_file_id: Option<u64>,
    _frn_to_path: &HashMap<u64, String>,
) -> UsnReadOutcome {
    UsnReadOutcome::HardError(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "USN journal is only available on Windows NTFS volumes",
    ))
}

#[cfg(not(windows))]
pub fn query_volume_details(_path: &Path) -> Option<VolumeDetails> {
    None
}

#[cfg(not(windows))]
pub fn supports_mft_scan(_path: &Path) -> bool {
    false
}

#[cfg(not(windows))]
pub fn enumerate_mft(_path: &Path) -> std::io::Result<Vec<MftEntry>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "MFT scan is only supported on Windows NTFS volumes",
    ))
}

#[cfg(not(windows))]
pub fn query_file_metadata_by_id(
    _path: &Path,
    _file_id: u64,
    _open_as_directory: bool,
) -> std::io::Result<FileIdMetadata> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "file-id metadata queries are only supported on Windows",
    ))
}

#[cfg(not(windows))]
pub fn enable_best_effort_scan_privileges() -> Vec<String> {
    Vec::new()
}

#[cfg(windows)]
mod windows_impl {
    use super::{
        FileIdMetadata, MftEntry, NativeDirEntry, UsnChangeSet, UsnJournalCheckpoint,
        UsnReadOutcome, UsnRenameEvent, VolumeDetails,
    };
    use std::collections::{HashMap, HashSet};
    use std::ffi::c_void;
    use std::iter::once;
    use std::mem::size_of;
    use std::path::{Path, PathBuf};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_HANDLE_EOF, FILETIME, GENERIC_READ, HANDLE, LUID,
    };
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FileBasicInfo, FileIdType, FileStandardInfo, FindClose, FindExInfoBasic,
        FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, GetFileInformationByHandle,
        GetFileInformationByHandleEx, GetVolumeInformationW, OpenFileById,
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_CREATION_DISPOSITION,
        FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_SHARE_DELETE, FILE_SHARE_MODE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FIND_FIRST_EX_LARGE_FETCH,
        OPEN_EXISTING, WIN32_FIND_DATAW,
    };
    use windows::Win32::System::Ioctl::{
        FSCTL_ENUM_USN_DATA, FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, MFT_ENUM_DATA_V0,
        READ_USN_JOURNAL_DATA_V0, USN_JOURNAL_DATA_V0, USN_RECORD_V2,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::System::IO::DeviceIoControl;

    const MFT_ENUM_BUFFER_SIZE: usize = 8 * 1024 * 1024;
    const USN_READ_BUFFER_SIZE: usize = 1024 * 1024;

    /// USN_REASON_* 位定义集中点。Windows SDK 头文件里每个常量定义在不同的地方
    /// （Ntifs.h / WinIoCtl.h），并不全部由 `windows` crate 暴露，且我们关心的
    /// reason 都属于稳定 NTFS USN_RECORD_V2 公开字段，按数值定义即可。
    ///
    /// 文档：https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v2
    #[allow(dead_code)]
    pub(super) mod usn_reason {
        pub const DATA_OVERWRITE: u32 = 0x0000_0001;
        pub const DATA_EXTEND: u32 = 0x0000_0002;
        pub const DATA_TRUNCATION: u32 = 0x0000_0004;
        pub const NAMED_DATA_OVERWRITE: u32 = 0x0000_0010;
        pub const NAMED_DATA_EXTEND: u32 = 0x0000_0020;
        pub const NAMED_DATA_TRUNCATION: u32 = 0x0000_0040;
        pub const FILE_CREATE: u32 = 0x0000_0100;
        pub const FILE_DELETE: u32 = 0x0000_0200;
        pub const EA_CHANGE: u32 = 0x0000_0400;
        pub const SECURITY_CHANGE: u32 = 0x0000_0800;
        pub const RENAME_OLD_NAME: u32 = 0x0000_1000;
        pub const RENAME_NEW_NAME: u32 = 0x0000_2000;
        pub const INDEXABLE_CHANGE: u32 = 0x0000_4000;
        pub const BASIC_INFO_CHANGE: u32 = 0x0000_8000;
        pub const HARD_LINK_CHANGE: u32 = 0x0001_0000;
        pub const COMPRESSION_CHANGE: u32 = 0x0002_0000;
        pub const ENCRYPTION_CHANGE: u32 = 0x0004_0000;
        pub const OBJECT_ID_CHANGE: u32 = 0x0008_0000;
        pub const REPARSE_POINT_CHANGE: u32 = 0x0010_0000;
        pub const STREAM_CHANGE: u32 = 0x0020_0000;
        pub const TRANSACTED_CHANGE: u32 = 0x0040_0000;
        pub const CLOSE: u32 = 0x8000_0000;

        /// 增量扫描真正关心的最小 reason 集：内容/创建/删除/改名/reparse。
        /// 写入前 / 中间过程不一定会触发 CLOSE，必要时单独检查。
        pub const TRACKED_MASK: u32 = DATA_OVERWRITE
            | DATA_EXTEND
            | DATA_TRUNCATION
            | NAMED_DATA_OVERWRITE
            | NAMED_DATA_EXTEND
            | NAMED_DATA_TRUNCATION
            | FILE_CREATE
            | FILE_DELETE
            | RENAME_OLD_NAME
            | RENAME_NEW_NAME
            | REPARSE_POINT_CHANGE
            | HARD_LINK_CHANGE
            | BASIC_INFO_CHANGE
            | CLOSE;
    }

    struct FindHandle(HANDLE);

    impl Drop for FindHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = FindClose(self.0);
            }
        }
    }

    pub(crate) struct OwnedHandle(pub(crate) HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn enable_best_effort_scan_privileges() -> Vec<String> {
        let mut token = HANDLE::default();
        let mut enabled = Vec::new();

        unsafe {
            if OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token,
            )
            .is_err()
            {
                return enabled;
            }
        }

        let _guard = OwnedHandle(token);
        for privilege_name in [
            "SeBackupPrivilege",
            "SeRestorePrivilege",
            "SeSecurityPrivilege",
            "SeManageVolumePrivilege",
        ] {
            if enable_named_privilege(token, privilege_name).is_ok() {
                enabled.push(privilege_name.to_string());
            }
        }

        enabled
    }

    fn enable_named_privilege(token: HANDLE, privilege_name: &str) -> std::io::Result<()> {
        let wide_name: Vec<u16> = privilege_name.encode_utf16().chain(once(0)).collect();
        let mut luid = LUID::default();

        unsafe {
            LookupPrivilegeValueW(None, PCWSTR(wide_name.as_ptr()), &mut luid)
                .map_err(to_io_error)?;
        }

        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };

        unsafe {
            AdjustTokenPrivileges(token, false, Some(&privileges), 0, None, None)
                .map_err(to_io_error)?;
        }

        Ok(())
    }

    pub fn enumerate_directory(
        path: &Path,
        include_dir_file_ids: bool,
    ) -> std::io::Result<Vec<NativeDirEntry>> {
        let search_pattern = path.join("*");
        let search_wide = to_wide(&search_pattern);
        let mut find_data = WIN32_FIND_DATAW::default();
        let handle = unsafe {
            FindFirstFileExW(
                PCWSTR(search_wide.as_ptr()),
                FindExInfoBasic,
                (&mut find_data as *mut WIN32_FIND_DATAW).cast::<c_void>(),
                FindExSearchNameMatch,
                None,
                FIND_FIRST_EX_LARGE_FETCH,
            )
        }
        .map_err(to_io_error)?;

        let _guard = FindHandle(handle);
        let mut entries = Vec::new();

        loop {
            let name = utf16_to_string(&find_data.cFileName);
            if name != "." && name != ".." {
                let entry_path = path.join(&name);
                let attributes = find_data.dwFileAttributes;
                let is_dir = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
                let is_symlink = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
                let file_id = if include_dir_file_ids && is_dir && !is_symlink {
                    get_path_file_id(&entry_path)
                } else {
                    None
                };

                entries.push(NativeDirEntry {
                    path: entry_path,
                    name,
                    size: ((find_data.nFileSizeHigh as u64) << 32) | find_data.nFileSizeLow as u64,
                    modified_time: filetime_to_unix(find_data.ftLastWriteTime),
                    is_dir,
                    is_symlink,
                    is_readonly: attributes & FILE_ATTRIBUTE_READONLY.0 != 0,
                    file_id,
                });
            }

            if unsafe { FindNextFileW(handle, &mut find_data) }.is_err() {
                break;
            }
        }

        Ok(entries)
    }

    pub fn get_path_file_id(path: &Path) -> Option<u64> {
        let wide = to_wide(path);
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                0,
                FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0),
                None,
                FILE_CREATION_DISPOSITION(OPEN_EXISTING.0),
                FILE_FLAGS_AND_ATTRIBUTES(
                    FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0,
                ),
                HANDLE::default(),
            )
        }
        .ok()?;

        let _guard = OwnedHandle(handle);
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(handle, &mut info) }.ok()?;
        Some(((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64)
    }

    pub fn query_volume_details(path: &Path) -> Option<VolumeDetails> {
        let volume_root = volume_root_path(path)?;
        let device_path = volume_device_path(&volume_root)?;
        let wide = to_wide(&volume_root);
        let mut file_system = vec![0u16; 64];

        unsafe {
            GetVolumeInformationW(
                PCWSTR(wide.as_ptr()),
                None,
                None,
                None,
                None,
                Some(&mut file_system),
            )
        }
        .ok()?;

        Some(VolumeDetails {
            volume_root,
            device_path,
            file_system: utf16_to_string(&file_system),
        })
    }

    pub fn supports_mft_scan(path: &Path) -> bool {
        query_volume_details(path)
            .filter(|details| details.file_system.eq_ignore_ascii_case("NTFS"))
            .and_then(|_| open_volume_handle_for_mft(path).ok())
            .map(|handle| {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                true
            })
            .unwrap_or(false)
    }

    pub fn enumerate_mft(path: &Path) -> std::io::Result<Vec<MftEntry>> {
        let volume = open_volume_handle_for_mft(path)?;
        let _guard = OwnedHandle(volume);
        let mut input = MFT_ENUM_DATA_V0 {
            StartFileReferenceNumber: 0,
            LowUsn: 0,
            HighUsn: i64::MAX,
        };
        let mut buffer = vec![0u8; MFT_ENUM_BUFFER_SIZE];
        let mut entries = Vec::new();

        loop {
            let mut output_bytes = 0u32;
            match unsafe {
                DeviceIoControl(
                    volume,
                    FSCTL_ENUM_USN_DATA,
                    Some((&input as *const MFT_ENUM_DATA_V0).cast::<c_void>()),
                    size_of::<MFT_ENUM_DATA_V0>() as u32,
                    Some(buffer.as_mut_ptr().cast::<c_void>()),
                    buffer.len() as u32,
                    Some(&mut output_bytes),
                    None,
                )
            } {
                Ok(_) => {}
                Err(err) if err.code() == ERROR_HANDLE_EOF.to_hresult() => break,
                Err(err) => return Err(to_io_error(err)),
            }

            if output_bytes <= size_of::<u64>() as u32 {
                break;
            }

            input.StartFileReferenceNumber = u64::from_ne_bytes(buffer[..8].try_into().unwrap());
            let mut offset = size_of::<u64>();
            while offset < output_bytes as usize {
                let header = unsafe { &*(buffer[offset..].as_ptr().cast::<USN_RECORD_V2>()) };
                if header.RecordLength == 0 {
                    break;
                }

                let record = unsafe { &*(buffer[offset..].as_ptr().cast::<USN_RECORD_V2>()) };
                let record_len = record.RecordLength as usize;
                if offset + record_len > output_bytes as usize {
                    break;
                }

                let attributes = record.FileAttributes;
                entries.push(MftEntry {
                    file_id: record.FileReferenceNumber,
                    parent_file_id: record.ParentFileReferenceNumber,
                    name: read_record_name(record, &buffer[offset..offset + record_len]),
                    is_dir: attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
                    is_reparse_point: attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
                    is_readonly: attributes & FILE_ATTRIBUTE_READONLY.0 != 0,
                });

                offset += record_len;
            }
        }

        Ok(entries)
    }

    pub fn query_file_metadata_by_id(
        path: &Path,
        file_id: u64,
        open_as_directory: bool,
    ) -> std::io::Result<FileIdMetadata> {
        let volume = open_volume_handle(path)?;
        let _guard = OwnedHandle(volume);
        query_file_metadata_by_id_on_volume(volume, file_id, open_as_directory)
    }

    pub(crate) fn query_file_metadata_by_id_on_volume(
        volume: HANDLE,
        file_id: u64,
        _open_as_directory: bool,
    ) -> std::io::Result<FileIdMetadata> {
        let file_id_desc = FILE_ID_DESCRIPTOR {
            dwSize: size_of::<FILE_ID_DESCRIPTOR>() as u32,
            Type: FileIdType,
            Anonymous: FILE_ID_DESCRIPTOR_0 {
                FileId: file_id as i64,
            },
        };
        let flags = FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0;
        let handle = unsafe {
            OpenFileById(
                volume,
                &file_id_desc,
                0,
                FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0),
                None,
                FILE_FLAGS_AND_ATTRIBUTES(flags),
            )
        }
        .map_err(to_io_error)?;

        let _guard = OwnedHandle(handle);
        let mut basic = FILE_BASIC_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileBasicInfo,
                (&mut basic as *mut FILE_BASIC_INFO).cast::<c_void>(),
                size_of::<FILE_BASIC_INFO>() as u32,
            )
        }
        .map_err(to_io_error)?;

        let mut standard = FILE_STANDARD_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileStandardInfo,
                (&mut standard as *mut FILE_STANDARD_INFO).cast::<c_void>(),
                size_of::<FILE_STANDARD_INFO>() as u32,
            )
        }
        .map_err(to_io_error)?;

        Ok(FileIdMetadata {
            size: standard.EndOfFile.max(0) as u64,
            modified_time: ticks_to_unix(basic.LastWriteTime),
            is_readonly: basic.FileAttributes & FILE_ATTRIBUTE_READONLY.0 != 0,
        })
    }

    pub fn query_usn_checkpoint(path: &Path) -> Option<UsnJournalCheckpoint> {
        let volume = match open_volume_handle(path) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::warn!(
                    "[winfs] failed to open volume for USN checkpoint {}: {}",
                    path.display(),
                    err
                );
                return None;
            }
        };
        let _guard = OwnedHandle(volume);
        let mut journal = USN_JOURNAL_DATA_V0::default();
        let mut bytes_returned = 0u32;

        if let Err(err) = unsafe {
            DeviceIoControl(
                volume,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some((&mut journal as *mut USN_JOURNAL_DATA_V0).cast::<c_void>()),
                size_of::<USN_JOURNAL_DATA_V0>() as u32,
                Some(&mut bytes_returned),
                None,
            )
        } {
            tracing::warn!(
                "[winfs] FSCTL_QUERY_USN_JOURNAL failed for {}: {}",
                path.display(),
                err
            );
            return None;
        }

        Some(UsnJournalCheckpoint {
            journal_id: journal.UsnJournalID,
            next_usn: journal.NextUsn,
        })
    }

    pub fn collect_usn_changed_dirs(
        root_path: &Path,
        checkpoint: UsnJournalCheckpoint,
        root_file_id: Option<u64>,
        frn_to_path: &HashMap<u64, String>,
    ) -> std::io::Result<Option<UsnChangeSet>> {
        match read_usn_journal_with_retry(root_path, checkpoint, root_file_id, frn_to_path) {
            UsnReadOutcome::Records(set) => Ok(Some(set)),
            UsnReadOutcome::JournalReset { .. } | UsnReadOutcome::StartUsnTooOld { .. } => Ok(None),
            UsnReadOutcome::HardError(err) => Err(err),
        }
    }

    /// 读 USN 日志的"权威"入口。相比 `collect_usn_changed_dirs`：
    ///
    /// - JournalReset / StartUsnTooOld 单独成枚举值，让上层有机会走"USN 全量重建"
    ///   而不是直接退化到 mtime 全量递归；
    /// - 顺手把 RENAME_OLD_NAME + RENAME_NEW_NAME 合并成单条 rename 事件，方便
    ///   增量合并阶段把"删旧 + 建新"识别成 rename，复用旧子树。
    pub fn read_usn_journal_with_retry(
        root_path: &Path,
        checkpoint: UsnJournalCheckpoint,
        root_file_id: Option<u64>,
        frn_to_path: &HashMap<u64, String>,
    ) -> UsnReadOutcome {
        let started = std::time::Instant::now();
        let volume = match open_volume_handle(root_path) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::warn!(
                    "[winfs] failed to open volume for USN read {}: {}",
                    root_path.display(),
                    err
                );
                return UsnReadOutcome::HardError(err);
            }
        };
        let _guard = OwnedHandle(volume);

        let mut current = USN_JOURNAL_DATA_V0::default();
        let mut bytes_returned = 0u32;
        if let Err(err) = unsafe {
            DeviceIoControl(
                volume,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some((&mut current as *mut USN_JOURNAL_DATA_V0).cast::<c_void>()),
                size_of::<USN_JOURNAL_DATA_V0>() as u32,
                Some(&mut bytes_returned),
                None,
            )
        } {
            tracing::warn!(
                "[winfs] FSCTL_QUERY_USN_JOURNAL failed before delta read for {}: {}",
                root_path.display(),
                err
            );
            return UsnReadOutcome::HardError(to_io_error(err));
        }

        if current.UsnJournalID != checkpoint.journal_id {
            tracing::info!(
                "[winfs-usn] journal_id changed path={} cached_journal_id={} current_journal_id={} first_usn={}",
                root_path.display(),
                checkpoint.journal_id,
                current.UsnJournalID,
                current.FirstUsn
            );
            return UsnReadOutcome::JournalReset {
                new_journal_id: current.UsnJournalID,
                first_usn: current.FirstUsn,
            };
        }

        if checkpoint.next_usn < current.FirstUsn {
            tracing::info!(
                "[winfs-usn] checkpoint older than first_usn path={} cached_next_usn={} first_usn={} current_next_usn={}",
                root_path.display(),
                checkpoint.next_usn,
                current.FirstUsn,
                current.NextUsn
            );
            return UsnReadOutcome::StartUsnTooOld {
                first_usn: current.FirstUsn,
            };
        }

        if current.NextUsn < checkpoint.next_usn {
            tracing::warn!(
                "[winfs-usn] current_next_usn={} smaller than checkpoint.next_usn={} path={}; treating as journal reset",
                current.NextUsn,
                checkpoint.next_usn,
                root_path.display()
            );
            return UsnReadOutcome::JournalReset {
                new_journal_id: current.UsnJournalID,
                first_usn: current.FirstUsn,
            };
        }

        let root_path_str = root_path.to_string_lossy().to_string();
        let mut input = READ_USN_JOURNAL_DATA_V0 {
            StartUsn: checkpoint.next_usn,
            // 只关心 TRACKED_MASK 范围内的变化；NTFS 会在内核侧帮忙过滤。
            ReasonMask: usn_reason::TRACKED_MASK,
            ReturnOnlyOnClose: 0,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: checkpoint.journal_id,
        };

        let mut root_files_changed = false;
        let mut recursive_dirs = HashSet::new();
        let mut direct_file_dirs = HashSet::new();
        let mut buffer = vec![0u8; USN_READ_BUFFER_SIZE];
        let mut read_calls = 0usize;
        let mut records_seen = 0usize;
        let mut rename_old: HashMap<u64, (String, bool)> = HashMap::new();
        let mut renames: Vec<UsnRenameEvent> = Vec::new();

        while input.StartUsn < current.NextUsn {
            read_calls += 1;
            let mut output_bytes = 0u32;
            if let Err(err) = unsafe {
                DeviceIoControl(
                    volume,
                    FSCTL_READ_USN_JOURNAL,
                    Some((&input as *const READ_USN_JOURNAL_DATA_V0).cast::<c_void>()),
                    size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
                    Some(buffer.as_mut_ptr().cast::<c_void>()),
                    buffer.len() as u32,
                    Some(&mut output_bytes),
                    None,
                )
            } {
                tracing::warn!(
                    "[winfs] FSCTL_READ_USN_JOURNAL failed for {} at start_usn={}: {}",
                    root_path.display(),
                    input.StartUsn,
                    err
                );
                // 中途读失败：再 query 一次 journal，区分 reset / 真硬错。
                let mut probe = USN_JOURNAL_DATA_V0::default();
                let mut probe_bytes = 0u32;
                let probe_ok = unsafe {
                    DeviceIoControl(
                        volume,
                        FSCTL_QUERY_USN_JOURNAL,
                        None,
                        0,
                        Some((&mut probe as *mut USN_JOURNAL_DATA_V0).cast::<c_void>()),
                        size_of::<USN_JOURNAL_DATA_V0>() as u32,
                        Some(&mut probe_bytes),
                        None,
                    )
                };
                if probe_ok.is_ok() {
                    if probe.UsnJournalID != checkpoint.journal_id {
                        return UsnReadOutcome::JournalReset {
                            new_journal_id: probe.UsnJournalID,
                            first_usn: probe.FirstUsn,
                        };
                    }
                    if input.StartUsn < probe.FirstUsn {
                        return UsnReadOutcome::StartUsnTooOld {
                            first_usn: probe.FirstUsn,
                        };
                    }
                }
                return UsnReadOutcome::HardError(to_io_error(err));
            }

            if output_bytes <= size_of::<i64>() as u32 {
                break;
            }

            input.StartUsn = i64::from_ne_bytes(buffer[..8].try_into().unwrap());
            let mut offset = size_of::<i64>();
            while offset < output_bytes as usize {
                let header = unsafe { &*(buffer[offset..].as_ptr().cast::<USN_RECORD_V2>()) };
                if header.RecordLength == 0 {
                    break;
                }

                let record = unsafe { &*(buffer[offset..].as_ptr().cast::<USN_RECORD_V2>()) };
                let record_len = record.RecordLength as usize;
                let is_dir = record.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
                let file_name = read_record_name(record, &buffer[offset..offset + record_len]);
                records_seen += 1;

                let parent_path = frn_to_path.get(&record.ParentFileReferenceNumber).cloned();
                let existing_path = frn_to_path.get(&record.FileReferenceNumber).cloned();
                let candidate_path = parent_path.as_ref().map(|parent| {
                    Path::new(parent)
                        .join(&file_name)
                        .to_string_lossy()
                        .to_string()
                });

                // 识别 rename：RENAME_OLD_NAME + RENAME_NEW_NAME 是同一次重命名
                // 触发的两条记录，pair-up 后视为 rename。
                let reason = record.Reason;
                if reason & usn_reason::RENAME_OLD_NAME != 0 {
                    if let Some(old_path) =
                        existing_path.clone().or_else(|| candidate_path.clone())
                    {
                        rename_old
                            .insert(record.FileReferenceNumber, (old_path, is_dir));
                    }
                }
                if reason & usn_reason::RENAME_NEW_NAME != 0 {
                    if let Some(new_path) = candidate_path.clone() {
                        if let Some((old_path, old_is_dir)) =
                            rename_old.remove(&record.FileReferenceNumber)
                        {
                            renames.push(UsnRenameEvent {
                                file_id: record.FileReferenceNumber,
                                old_path,
                                new_path,
                                is_dir: is_dir || old_is_dir,
                            });
                        }
                    }
                }

                if !is_dir {
                    if let Some(parent) = parent_path.as_ref() {
                        if parent == &root_path_str {
                            root_files_changed = true;
                        } else {
                            direct_file_dirs.insert(parent.clone());
                        }
                    } else if Some(record.ParentFileReferenceNumber) == root_file_id {
                        root_files_changed = true;
                    }
                } else {
                    if let Some(path) = existing_path {
                        recursive_dirs.insert(path);
                    }
                    if let Some(path) = candidate_path {
                        recursive_dirs.insert(path);
                    }
                }

                offset += record_len;
            }
        }

        tracing::debug!("[scan-timing][winfs-usn] read_usn_journal_with_retry path={} took {:.2}ms | read_calls={} records_seen={} recursive_dirs={} direct_file_dirs={} root_files_changed={} renames={} start_usn={} end_usn={}",
            root_path.display(),
            started.elapsed().as_secs_f64() * 1000.0,
            read_calls,
            records_seen,
            recursive_dirs.len(),
            direct_file_dirs.len(),
            root_files_changed,
            renames.len(),
            checkpoint.next_usn,
            current.NextUsn
        );

        UsnReadOutcome::Records(UsnChangeSet {
            recursive_dirs,
            direct_file_dirs,
            root_files_changed,
            renames,
        })
    }

    fn read_record_name(record: &USN_RECORD_V2, record_bytes: &[u8]) -> String {
        let start = record.FileNameOffset as usize;
        let end = start + record.FileNameLength as usize;
        if end > record_bytes.len() || start >= end {
            return String::new();
        }

        let wide_len = (end - start) / 2;
        let wide = unsafe {
            std::slice::from_raw_parts(record_bytes[start..end].as_ptr().cast::<u16>(), wide_len)
        };
        String::from_utf16_lossy(wide)
    }

    pub(crate) fn open_volume_handle(path: &Path) -> std::io::Result<HANDLE> {
        open_volume_handle_with_access(path, GENERIC_READ.0)
    }

    fn open_volume_handle_with_access(path: &Path, desired_access: u32) -> std::io::Result<HANDLE> {
        let volume_path = volume_device_path(path).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path is not on a local volume",
            )
        })?;
        let wide = to_wide(Path::new(&volume_path));
        unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                desired_access,
                FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0),
                None,
                FILE_CREATION_DISPOSITION(OPEN_EXISTING.0),
                FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS.0),
                HANDLE::default(),
            )
        }
        .map_err(to_io_error)
    }

    fn open_volume_handle_for_mft(path: &Path) -> std::io::Result<HANDLE> {
        open_volume_handle_with_access(path, GENERIC_READ.0)
    }

    fn volume_device_path(path: &Path) -> Option<String> {
        let path_str = path.to_string_lossy();
        let bytes = path_str.as_bytes();
        if bytes.len() < 2 || bytes[1] != b':' {
            return None;
        }
        Some(format!(r"\\.\{}:", bytes[0] as char))
    }

    fn volume_root_path(path: &Path) -> Option<PathBuf> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::fs::canonicalize(path).ok()?
        };
        let path_str = absolute.to_string_lossy();
        let bytes = path_str.as_bytes();
        if bytes.len() < 2 || bytes[1] != b':' {
            return None;
        }
        Some(PathBuf::from(format!(r"{}:\", bytes[0] as char)))
    }

    fn filetime_to_unix(filetime: FILETIME) -> Option<u64> {
        let ticks = ((filetime.dwHighDateTime as u64) << 32) | filetime.dwLowDateTime as u64;
        ticks_to_unix(ticks as i64)
    }

    fn ticks_to_unix(ticks: i64) -> Option<u64> {
        if ticks <= 0 {
            return None;
        }
        if ticks == 0 {
            return None;
        }
        (ticks as u64)
            .checked_sub(116_444_736_000_000_000)
            .map(|value| value / 10_000_000)
    }

    fn utf16_to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|&ch| ch == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    fn to_wide(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;

        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn to_io_error(err: windows::core::Error) -> std::io::Error {
        std::io::Error::other(err.to_string())
    }
}

#[cfg(windows)]
pub use windows_impl::{
    collect_usn_changed_dirs, enable_best_effort_scan_privileges, enumerate_directory,
    enumerate_mft, get_path_file_id, query_file_metadata_by_id, query_usn_checkpoint,
    query_volume_details, read_usn_journal_with_retry, supports_mft_scan,
};
#[cfg(windows)]
pub(crate) use windows_impl::{open_volume_handle, query_file_metadata_by_id_on_volume};
