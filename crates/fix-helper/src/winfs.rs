//! File-system primitives that are safe to use from an elevated process on
//! directories a non-admin user can write to: links are never followed at
//! the final component, every deletion is verified through the handle's
//! final path, and known folders come from APIs, not environment variables.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

/// True when `child` lies strictly inside `root` (case-insensitive, both
/// already in the same normalized form, e.g. final paths).
pub fn path_is_within(child: &str, root: &str) -> bool {
    let root = root.trim_end_matches('\\').to_lowercase();
    let child = child.to_lowercase();
    !root.is_empty()
        && child.len() > root.len() + 1
        && child.starts_with(&root)
        && child[root.len()..].starts_with('\\')
}

#[cfg(not(windows))]
pub fn open_no_follow(path: &Path, truncate: bool) -> io::Result<File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(truncate)
        .open(path)
}

#[cfg(windows)]
pub use imp::*;

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{BOOLEAN, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        FileDispositionInfo, FileDispositionInfoEx, GetFileInformationByHandle,
        GetFinalPathNameByHandleW, SetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
        FILE_DISPOSITION_INFO_EX_FLAGS, GETFINALPATHNAMEBYHANDLE_FLAGS,
    };

    pub const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const DELETE: u32 = 0x0001_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const SHARE_ALL: u32 = 0x7;

    fn handle(f: &File) -> HANDLE {
        HANDLE(f.as_raw_handle() as _)
    }

    fn info(f: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
        let mut i = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(handle(f), &mut i) }.map_err(io::Error::other)?;
        Ok(i)
    }

    /// Final (link-resolved, normalized) path of an open handle, `\\?\C:\...`.
    pub fn final_path_of(f: &File) -> io::Result<String> {
        let mut buf = vec![0u16; 1024];
        loop {
            let n = unsafe {
                GetFinalPathNameByHandleW(handle(f), &mut buf, GETFINALPATHNAMEBYHANDLE_FLAGS(0))
            } as usize;
            if n == 0 {
                return Err(io::Error::last_os_error());
            }
            if n < buf.len() {
                return Ok(String::from_utf16_lossy(&buf[..n]));
            }
            if buf.len() > 65_536 {
                return Err(io::Error::other("path too long"));
            }
            buf.resize(n + 1, 0);
        }
    }

    /// Final path of `path` (links along the way ARE resolved, which is the
    /// point: the result is where the object really is).
    pub fn final_path(path: &Path) -> io::Result<String> {
        let f = std::fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        final_path_of(&f)
    }

    /// Opens `path` for writing without following a link at the final
    /// component, creating it if needed. Refuses links, hard-linked files and
    /// non-files BEFORE any byte is changed; truncates only after the checks.
    pub fn open_no_follow(path: &Path, truncate: bool) -> io::Result<File> {
        let f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0x1) // FILE_SHARE_READ
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let i = info(&f)?;
        if i.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0 {
            return Err(io::Error::other("refusing to write through a link"));
        }
        if i.nNumberOfLinks != 1 {
            return Err(io::Error::other("refusing to write a hard-linked file"));
        }
        if truncate {
            f.set_len(0)?;
        }
        Ok(f)
    }

    /// True for symlinks, junctions and every other reparse point.
    pub fn is_reparse(md: &std::fs::Metadata) -> bool {
        md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || md.file_type().is_symlink()
    }

    /// Deletes `path` (a file or an empty directory) only if, seen through
    /// its own handle, it is not a reparse point and its final path is
    /// strictly inside `root_final`. The handle is opened without following
    /// a final-component link; deletion goes through that same handle, so a
    /// swap after the check cannot redirect it.
    pub fn delete_verified(path: &Path, root_final: &str) -> io::Result<()> {
        let f = std::fs::OpenOptions::new()
            .access_mode(DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        if info(&f)?.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other("reparse point"));
        }
        let fp = final_path_of(&f)?;
        if !path_is_within(&fp, root_final) {
            return Err(io::Error::other(format!("outside the allowed root: {fp}")));
        }
        let ex = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
                FILE_DISPOSITION_FLAG_DELETE.0
                    | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                    | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
            ),
        };
        let r = unsafe {
            SetFileInformationByHandle(
                handle(&f),
                FileDispositionInfoEx,
                &ex as *const _ as *const _,
                std::mem::size_of_val(&ex) as u32,
            )
        };
        if r.is_ok() {
            return Ok(());
        }
        // Older Windows without FileDispositionInfoEx.
        let basic = FILE_DISPOSITION_INFO {
            DeleteFile: BOOLEAN(1),
        };
        unsafe {
            SetFileInformationByHandle(
                handle(&f),
                FileDispositionInfo,
                &basic as *const _ as *const _,
                std::mem::size_of_val(&basic) as u32,
            )
        }
        .map_err(io::Error::other)
    }

    /// `ProgramData` via `SHGetKnownFolderPath`.
    pub fn program_data() -> Option<PathBuf> {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::{
            FOLDERID_ProgramData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
        };
        let p = unsafe {
            SHGetKnownFolderPath(&FOLDERID_ProgramData, KF_FLAG_DEFAULT, HANDLE::default())
        }
        .ok()?;
        let s = unsafe { p.to_string() }.ok();
        unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
        s.filter(|s| !s.is_empty()).map(PathBuf::from)
    }

    /// The current user's profile directory from the token (HKLM ProfileList,
    /// not `%USERPROFILE%`).
    pub fn user_profile_dir() -> io::Result<PathBuf> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Security::TOKEN_QUERY;
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        use windows::Win32::UI::Shell::GetUserProfileDirectoryW;
        let mut tok = HANDLE::default();
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) }
            .map_err(io::Error::other)?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let r = unsafe {
            GetUserProfileDirectoryW(tok, windows::core::PWSTR(buf.as_mut_ptr()), &mut len)
        };
        let _ = unsafe { CloseHandle(tok) };
        r.map_err(io::Error::other)?;
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
    }

    /// Wide string helper for registry/service APIs.
    pub fn wide(s: &str) -> HSTRING {
        HSTRING::from(s)
    }

    pub fn pcwstr(h: &HSTRING) -> PCWSTR {
        PCWSTR(h.as_ptr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_is_strict_and_case_insensitive() {
        let root = r"\\?\C:\Users\Ali\AppData\Local\Temp";
        assert!(path_is_within(
            r"\\?\C:\Users\Ali\AppData\Local\Temp\a.txt",
            root
        ));
        assert!(path_is_within(
            r"\\?\c:\users\ali\appdata\local\temp\x\y",
            root
        ));
        assert!(path_is_within(
            r"\\?\C:\Users\Ali\AppData\Local\Temp\a",
            &format!("{root}\\")
        ));
        assert!(!path_is_within(root, root));
        assert!(!path_is_within(&format!("{root}\\"), root));
        assert!(!path_is_within(
            r"\\?\C:\Users\Ali\AppData\Local\Temp2\a",
            root
        ));
        assert!(!path_is_within(r"\\?\C:\Windows\System32\x", root));
        assert!(!path_is_within(r"\\?\C:\x", ""));
    }

    #[cfg(windows)]
    #[test]
    fn open_no_follow_refuses_hard_links_before_truncating() {
        let dir = std::env::temp_dir().join(format!("fix-helper-hl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.txt");
        std::fs::write(&target, b"keep me").unwrap();
        std::fs::hard_link(&target, dir.join("link.json")).unwrap();
        assert!(open_no_follow(&dir.join("link.json"), true).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
        let plain = open_no_follow(&dir.join("plain.json"), true);
        assert!(plain.is_ok());
        drop(plain);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn known_folders_resolve() {
        assert!(program_data().unwrap().is_absolute());
        assert!(user_profile_dir().unwrap().is_dir());
    }
}
