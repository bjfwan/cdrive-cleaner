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
}

impl UsnChangeSet {
    pub fn all_changed_dirs(&self) -> HashSet<String> {
        let mut all = self.recursive_dirs.clone();
        all.extend(self.direct_file_dirs.iter().cloned());
        all
    }
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
        FileIdMetadata, MftEntry, NativeDirEntry, UsnChangeSet, UsnJournalCheckpoint, VolumeDetails,
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

        let mut privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };

        unsafe {
            AdjustTokenPrivileges(token, false, Some(&mut privileges), 0, None, None)
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
        open_as_directory: bool,
    ) -> std::io::Result<FileIdMetadata> {
        let file_id_desc = FILE_ID_DESCRIPTOR {
            dwSize: size_of::<FILE_ID_DESCRIPTOR>() as u32,
            Type: FileIdType,
            Anonymous: FILE_ID_DESCRIPTOR_0 {
                FileId: file_id as i64,
            },
        };
        let flags = if open_as_directory {
            FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0
        } else {
            FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0
        };
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
        let started = std::time::Instant::now();
        let volume = match open_volume_handle(root_path) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::warn!(
                    "[winfs] failed to open volume for USN read {}: {}",
                    root_path.display(),
                    err
                );
                return Ok(None);
            }
        };
        let _guard = OwnedHandle(volume);

        let mut current = USN_JOURNAL_DATA_V0::default();
        let mut bytes_returned = 0u32;
        if unsafe {
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
        }
        .is_err()
        {
            tracing::warn!(
                "[winfs] FSCTL_QUERY_USN_JOURNAL failed before delta read for {}",
                root_path.display()
            );
            return Ok(None);
        }

        if current.UsnJournalID != checkpoint.journal_id || current.NextUsn < checkpoint.next_usn {
            tracing::debug!("[scan-timing][winfs-usn] collect_usn_changed_dirs path={} took {:.2}ms | status=checkpoint_invalid current_journal_id={} checkpoint_journal_id={} current_next_usn={} checkpoint_next_usn={}",
                root_path.display(),
                started.elapsed().as_secs_f64() * 1000.0,
                current.UsnJournalID,
                checkpoint.journal_id,
                current.NextUsn,
                checkpoint.next_usn
            );
            return Ok(None);
        }

        let root_path_str = root_path.to_string_lossy().to_string();
        let mut input = READ_USN_JOURNAL_DATA_V0 {
            StartUsn: checkpoint.next_usn,
            ReasonMask: u32::MAX,
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

        while input.StartUsn < current.NextUsn {
            read_calls += 1;
            let mut output_bytes = 0u32;
            if unsafe {
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
            }
            .is_err()
            {
                tracing::warn!(
                    "[winfs] FSCTL_READ_USN_JOURNAL failed for {} at start_usn={}",
                    root_path.display(),
                    input.StartUsn
                );
                return Ok(None);
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

        tracing::debug!("[scan-timing][winfs-usn] collect_usn_changed_dirs path={} took {:.2}ms | read_calls={} records_seen={} recursive_dirs={} direct_file_dirs={} root_files_changed={} start_usn={} end_usn={}",
            root_path.display(),
            started.elapsed().as_secs_f64() * 1000.0,
            read_calls,
            records_seen,
            recursive_dirs.len(),
            direct_file_dirs.len(),
            root_files_changed,
            checkpoint.next_usn,
            current.NextUsn
        );

        Ok(Some(UsnChangeSet {
            recursive_dirs,
            direct_file_dirs,
            root_files_changed,
        }))
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
        std::io::Error::new(std::io::ErrorKind::Other, err.to_string())
    }
}

#[cfg(windows)]
pub use windows_impl::{
    collect_usn_changed_dirs, enable_best_effort_scan_privileges, enumerate_directory,
    enumerate_mft, get_path_file_id, query_file_metadata_by_id, query_usn_checkpoint,
    query_volume_details, supports_mft_scan,
};
#[cfg(windows)]
pub(crate) use windows_impl::{open_volume_handle, query_file_metadata_by_id_on_volume};
