# MFT + USN Backend Architecture

## Goal

The scanner now has a Windows-first backend split:

- `native` backend: recursive directory enumeration, used as the safe fallback
- `mft_usn` backend: NTFS-only, uses MFT enumeration for full metadata traversal and USN checkpoints for incremental updates

This keeps the project usable in normal user sessions while allowing an elevated build to switch to the higher-performance path.

## Runtime Architecture

```text
┌─────────────────────────────┐
│ commands.rs                 │
│ Tauri IPC entrypoints       │
└─────────────┬───────────────┘
              │
┌─────────────▼───────────────┐
│ scanner/disk_scanner.rs     │
│ orchestration + fallback    │
└──────┬───────────────┬──────┘
       │               │
       │               │
┌──────▼───────┐ ┌─────▼──────────────┐
│ native path  │ │ scanner/mft_usn.rs │
│ FindFirst... │ │ MFT hydration      │
│ recursive    │ │ pure Rust backend  │
└──────┬───────┘ └─────┬──────────────┘
       │               │
       └──────┬────────┘
              │
┌─────────────▼───────────────┐
│ winfs.rs                    │
│ Windows primitives          │
│ - FindFirstFileExW          │
│ - FSCTL_ENUM_USN_DATA       │
│ - FSCTL_QUERY_USN_JOURNAL   │
│ - FSCTL_READ_USN_JOURNAL    │
│ - OpenFileById              │
└─────────────────────────────┘
```

## File Ownership

- `cdrive-cleaner/src-tauri/src/scanner/backend.rs`
  Chooses `native` vs `mft_usn` based on NTFS support and current volume access.

- `cdrive-cleaner/src-tauri/src/scanner/mft_usn.rs`
  Implements the NTFS backend. Responsibilities:
  - enumerate all MFT records under the volume
  - filter descendants of the requested root by file reference number
  - hydrate file sizes and write times through `OpenFileById`
  - assemble `DirectoryNode` trees and `large_files`
  - stay GUI-agnostic by using a Rust progress callback instead of a Tauri handle

- `cdrive-cleaner/src-tauri/src/scanner/progress.rs`
  Shared scan progress payload. This lets the backend run without a GUI runtime.

- `cdrive-cleaner/src-tauri/src/winfs.rs`
  Windows-only filesystem bridge. Responsibilities:
  - shallow directory enumeration
  - USN checkpoint and delta collection
  - NTFS volume probing
  - MFT enumeration
  - file-id metadata lookup through `OpenFileById`

- `cdrive-cleaner/src-tauri/src/scanner/disk_scanner.rs`
  Keeps the public scanning flow stable. It now:
  - asks `backend.rs` which scanner to use
  - runs `mft_usn` when the NTFS volume can be opened for MFT access
  - falls back to the existing native recursive path on unsupported or non-elevated sessions

- `cdrive-cleaner/src-tauri/src/scanner/incremental.rs`
  Still owns the merge/update layer. It now keeps explicit backend provenance in `ScanResult`.

- `cdrive-cleaner/src-tauri/src/migration/file_migrator.rs`
  Progress emission was also decoupled from Tauri into a Rust callback so migration can be validated independently from the GUI runtime.

## Full Scan Flow

```text
1. Check volume type and privileges
2. If NTFS + MFT access available:
   - enumerate MFT via FSCTL_ENUM_USN_DATA
   - resolve descendant FRNs under the requested root
   - hydrate files/dirs via OpenFileById
   - build aggregated directory tree
   - store USN checkpoint in ScanResult
3. Otherwise:
   - fall back to native recursive enumeration
```

## Incremental Flow

```text
1. Read cached ScanResult
2. Use stored root FRN + USN checkpoint
3. Collect changed directories from FSCTL_READ_USN_JOURNAL
4. Rescan only changed subtrees
5. Merge delta back into cached directory tree
```

## Privilege Model

`MFT` access is not treated as universally available.

- `supports_mft_scan(path)` now means:
  - the volume is `NTFS`
  - the current process can actually open the volume for MFT enumeration

- If either condition fails, the scanner intentionally falls back to `native`.

That means:

- non-admin sessions remain functional
- elevated sessions can use the fast path
- the app no longer throws a hard failure just because the shell is not elevated

## Validation Notes

Real validation was split into two parts:

- compile validation: `cargo test --no-run`
- filesystem validation: temporary real-directory migration checks plus a dedicated admin-only `MFT + USN` end-to-end validation command

### Admin End-to-End Validation

From the repository root, the self-elevating command is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-admin-mft-validation.ps1 C:\
```

Behavior:

- if the shell is not elevated, the script re-launches itself through Windows UAC
- after elevation, it runs `cargo run --example admin_mft_validation -- C:\`
- the example prints a JSON report and exits with code `2` when validation does not pass
- the temporary validation directory is deleted automatically on exit

If you are already in an elevated shell, you can run the example directly:

```powershell
cd .\cdrive-cleaner\src-tauri
cargo run --example admin_mft_validation -- C:\
```

### User-Facing Admin Path

The app now exposes the admin path in two places:

- `设置 -> 权限 -> 管理员模式`
- the main scan action card, which now detects whether the selected NTFS volume can use `MFT + USN` and shows a one-click `开启管理员模式` action when elevation is recommended

When the restart prompt appears, the user needs to click `是` in the Windows UAC dialog. After restart, deep scan will prefer the `MFT + USN` backend whenever the target volume supports it.
