//! Files only the current user can read (SPEC §21.1; STATE.md F5.3;
//! ADR-0024).
//!
//! A cookie file's working copy is plaintext for the length of one job
//! (yt-dlp has to read it). It lives in a folder the app creates with an
//! access list that names only the current user:
//!
//! - Windows: a protected DACL with one entry, full control for the user's
//!   own SID, inherited by every file inside. Administrators, SYSTEM and
//!   other users get no entry (an administrator can still take ownership;
//!   that is the OS's boundary, not ours).
//! - Unix: folder `0700`, files `0600`.
//!
//! Files are created with `create_new`, so an existing file (or a link put
//! there) is never followed or overwritten.

use std::io::Write;
use std::path::Path;

/// Creates `dir` (and its parents) with an access list for the current user
/// only. An existing folder is left as it is: the app made it the same way.
pub fn create_dir(dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    imp::create_dir(dir)
}

/// Writes a new file readable only by the current user. Fails if the file
/// exists.
pub fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = imp::open_new(path)?;
    let r = f.write_all(bytes).and_then(|_| f.sync_all());
    if r.is_err() {
        drop(f);
        let _ = std::fs::remove_file(path);
    }
    r
}

#[cfg(unix)]
mod imp {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use std::path::Path;

    pub fn create_dir(dir: &Path) -> std::io::Result<()> {
        match std::fs::DirBuilder::new().mode(0o700).create(dir) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && dir.is_dir() => Ok(()),
            r => r,
        }
    }

    pub fn open_new(path: &Path) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn last_error() -> std::io::Error {
        std::io::Error::last_os_error()
    }

    /// `S-1-5-21-…` of the user this process runs as.
    fn user_sid() -> std::io::Result<String> {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(last_error());
            }
            let mut len = 0u32;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len);
            let mut buf = vec![0u8; len.max(64) as usize];
            let ok = GetTokenInformation(
                token,
                TokenUser,
                buf.as_mut_ptr() as *mut c_void,
                buf.len() as u32,
                &mut len,
            );
            let err = (ok == 0).then(last_error);
            CloseHandle(token);
            if let Some(e) = err {
                return Err(e);
            }
            let tu = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(tu.User.Sid, &mut s) == 0 {
                return Err(last_error());
            }
            let mut n = 0;
            while *s.add(n) != 0 {
                n += 1;
            }
            let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s as _);
            Ok(out)
        }
    }

    pub fn create_dir(dir: &Path) -> std::io::Result<()> {
        // Protected (no inheritance from the parent), one entry: full
        // control for this user, inherited by files and folders inside.
        let sddl = format!("D:P(A;OICI;FA;;;{})", user_sid()?);
        let sddl_w = wide(std::ffi::OsStr::new(&sddl));
        unsafe {
            let mut sd: *mut c_void = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl_w.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(last_error());
            }
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd,
                bInheritHandle: 0,
            };
            let path = wide(dir.as_os_str());
            let ok = CreateDirectoryW(path.as_ptr(), &sa);
            let err = (ok == 0).then(last_error);
            LocalFree(sd as _);
            match err {
                None => Ok(()),
                Some(e) if e.kind() == std::io::ErrorKind::AlreadyExists && dir.is_dir() => Ok(()),
                Some(e) => Err(e),
            }
        }
    }

    pub fn open_new(path: &Path) -> std::io::Result<std::fs::File> {
        // The file inherits the folder's single entry.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_files_are_new_only_and_owner_only() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("a").join("cookies");
        create_dir(&dir).unwrap();
        create_dir(&dir).unwrap();
        let f = dir.join("x.txt");
        write_new(&f, b"hello").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"hello");
        assert!(write_new(&f, b"again").is_err(), "never overwrites");
        assert_eq!(std::fs::read(&f).unwrap(), b"hello");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(m(&dir), 0o700);
            assert_eq!(m(&f), 0o600);
        }
    }
}
