use std::collections::HashSet;
use std::path::PathBuf;

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
    pub changed_dirs: HashSet<String>,
    pub root_files_changed: bool,
}

#[cfg(not(windows))]
use std::collections::{HashMap, HashSet};
#[cfg(not(windows))]
use std::path::Path;

#[cfg(not(windows))]
pub fn enumerate_directory(path: &Path, include_dir_file_ids: bool) -> std::io::Result<Vec<NativeDirEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = std::fs::symlink_metadata(&entry_path)?;
        let is_symlink = metadata.file_type().is_symlink();
        let is_dir = metadata.is_dir();
        let modified_time = metadata.modified().ok()
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
            file_id: include_dir_file_ids && is_dir && !is_symlink
                .then_some(0)
                .filter(|_| false),
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

#[cfg(windows)]
mod windows_impl {
    use super::{NativeDirEntry, UsnChangeSet, UsnJournalCheckpoint};
    use std::collections::{HashMap, HashSet};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::path::Path;
    use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_CREATION_DISPOSITION,
        FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FIND_FIRST_EX_LARGE_FETCH, FindClose, FindExInfoBasic,
        FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, GetFileInformationByHandle,
        OPEN_EXISTING, WIN32_FIND_DATAW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0,
        USN_JOURNAL_DATA_V0, USN_RECORD_V2,
    };
    use windows::core::PCWSTR;

    struct FindHandle(HANDLE);

    impl Drop for FindHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = FindClose(self.0);
            }
        }
    }

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn enumerate_directory(path: &Path, include_dir_file_ids: bool) -> std::io::Result<Vec<NativeDirEntry>> {
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
                FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0),
                HANDLE::default(),
            )
        }
        .ok()?;

        let _guard = OwnedHandle(handle);
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(handle, &mut info) }.ok()?;
        Some(((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64)
    }

    pub fn query_usn_checkpoint(path: &Path) -> Option<UsnJournalCheckpoint> {
        let volume = open_volume_handle(path).ok()?;
        let _guard = OwnedHandle(volume);
        let mut journal = USN_JOURNAL_DATA_V0::default();
        let mut bytes_returned = 0u32;

        unsafe {
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
        }
        .ok()?;

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
        let volume = match open_volume_handle(root_path) {
            Ok(handle) => handle,
            Err(_) => return Ok(None),
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
            return Ok(None);
        }

        if current.UsnJournalID != checkpoint.journal_id || current.NextUsn < checkpoint.next_usn {
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
        let mut changed_dirs = HashSet::new();
        let mut buffer = vec![0u8; 256 * 1024];

        while input.StartUsn < current.NextUsn {
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

                let parent_path = frn_to_path.get(&record.ParentFileReferenceNumber).cloned();
                let existing_path = frn_to_path.get(&record.FileReferenceNumber).cloned();
                let candidate_path = parent_path
                    .as_ref()
                    .map(|parent| Path::new(parent).join(&file_name).to_string_lossy().to_string());

                if !is_dir {
                    if let Some(parent) = parent_path.as_ref() {
                        changed_dirs.insert(parent.clone());
                        if parent == &root_path_str {
                            root_files_changed = true;
                        }
                    } else if Some(record.ParentFileReferenceNumber) == root_file_id {
                        root_files_changed = true;
                    }
                } else {
                    if let Some(path) = existing_path {
                        changed_dirs.insert(path);
                    }
                    if let Some(path) = candidate_path {
                        changed_dirs.insert(path);
                    }
                    if let Some(parent) = parent_path {
                        changed_dirs.insert(parent);
                    }
                }

                offset += record_len;
            }
        }

        Ok(Some(UsnChangeSet {
            changed_dirs,
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

    fn open_volume_handle(path: &Path) -> std::io::Result<HANDLE> {
        let volume_path = volume_device_path(path).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is not on a local volume")
        })?;
        let wide = to_wide(Path::new(&volume_path));
        unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                0,
                FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0),
                None,
                FILE_CREATION_DISPOSITION(OPEN_EXISTING.0),
                FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS.0),
                HANDLE::default(),
            )
        }
        .map_err(to_io_error)
    }

    fn volume_device_path(path: &Path) -> Option<String> {
        let path_str = path.to_string_lossy();
        let bytes = path_str.as_bytes();
        if bytes.len() < 2 || bytes[1] != b':' {
            return None;
        }
        Some(format!(r"\\.\{}:", bytes[0] as char))
    }

    fn filetime_to_unix(filetime: FILETIME) -> Option<u64> {
        let ticks = ((filetime.dwHighDateTime as u64) << 32) | filetime.dwLowDateTime as u64;
        if ticks == 0 {
            return None;
        }
        ticks
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
pub use windows_impl::{collect_usn_changed_dirs, enumerate_directory, get_path_file_id, query_usn_checkpoint};
