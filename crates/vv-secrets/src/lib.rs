//! VideoVault secret store (SPEC §11.6, §20.3; STATE.md F5.1; ADR-0022).
//!
//! A secret lives only here, behind an opaque reference
//! (`AuthProfile.secure_local_ref`). The database, the job payload, logs and
//! the UI see the reference, never the value.
//!
//! - [`DpapiStore`] (Windows): each secret is encrypted with DPAPI for the
//!   current Windows user and written to `<dir>/<ref>.vvsec`. Another user,
//!   or the same file on another machine, cannot decrypt it.
//! - [`MemoryStore`]: tests only; nothing touches the disk.
//! - [`UnavailableStore`]: platforms without an OS backend. Every write
//!   fails, so the app can say "unsupported" instead of storing plaintext.
//!
//! There is deliberately no plaintext file backend.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

/// Bytes that must not leak. `Debug` never prints them and the buffer is
/// overwritten when dropped.
pub struct Secret(Vec<u8>);

impl Secret {
    pub fn new(bytes: Vec<u8>) -> Self {
        Secret(bytes)
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        for b in self.0.iter_mut() {
            // Volatile so the compiler cannot drop the "dead" stores.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret([redacted; {} bytes])", self.0.len())
    }
}

/// Opaque reference to a stored secret. Safe to put in the database: it is
/// a random id, not derived from the value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretRef(String);

impl SecretRef {
    /// Fresh random reference (`vvsec:` + UUID v4).
    pub fn generate() -> Self {
        SecretRef(format!("{PREFIX}{}", uuid::Uuid::new_v4()))
    }

    /// Accepts only the shape [`generate`](Self::generate) makes, so a stored
    /// value can never steer the store to another path.
    pub fn parse(s: &str) -> Option<Self> {
        let id = s.strip_prefix(PREFIX)?;
        uuid::Uuid::parse_str(id).ok()?;
        (id.len() == 36 && id == id.to_ascii_lowercase()).then(|| SecretRef(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn file_stem(&self) -> &str {
        &self.0[PREFIX.len()..]
    }
}

const PREFIX: &str = "vvsec:";

/// Largest secret accepted. Platform-scoped cookies are a few KiB; anything
/// much larger is not what this store is for.
pub const MAX_SECRET_BYTES: usize = 256 * 1024;

#[derive(Debug)]
pub enum SecretError {
    /// No OS-protected backend on this platform.
    Unavailable,
    NotFound,
    TooLarge,
    /// The OS refused to protect or unprotect the data (DPAPI error code).
    Os(i32),
    Io(std::io::Error),
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretError::Unavailable => write!(f, "no OS secret store on this platform"),
            SecretError::NotFound => write!(f, "secret not found"),
            SecretError::TooLarge => write!(f, "secret too large"),
            SecretError::Os(code) => write!(f, "OS secret protection failed (code {code})"),
            // Never include the path: it names the user's profile folder.
            SecretError::Io(e) => write!(f, "secret store I/O failed ({:?})", e.kind()),
        }
    }
}

impl std::error::Error for SecretError {}

pub type Result<T> = std::result::Result<T, SecretError>;

pub trait SecretStore: Send + Sync {
    /// Stores `secret` under a new reference.
    fn put(&self, secret: &Secret) -> Result<SecretRef>;
    fn get(&self, r: &SecretRef) -> Result<Secret>;
    /// Removes the secret. Removing a missing secret is not an error.
    fn delete(&self, r: &SecretRef) -> Result<()>;
    /// Whether this store really protects data (false for
    /// [`UnavailableStore`]).
    fn available(&self) -> bool {
        true
    }
}

/// Tests only. Keeps secrets in process memory.
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<HashMap<SecretRef, Vec<u8>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SecretStore for MemoryStore {
    fn put(&self, secret: &Secret) -> Result<SecretRef> {
        if secret.len() > MAX_SECRET_BYTES {
            return Err(SecretError::TooLarge);
        }
        let r = SecretRef::generate();
        self.items
            .lock()
            .unwrap()
            .insert(r.clone(), secret.expose().to_vec());
        Ok(r)
    }

    fn get(&self, r: &SecretRef) -> Result<Secret> {
        self.items
            .lock()
            .unwrap()
            .get(r)
            .map(|v| Secret::new(v.clone()))
            .ok_or(SecretError::NotFound)
    }

    fn delete(&self, r: &SecretRef) -> Result<()> {
        if let Some(mut v) = self.items.lock().unwrap().remove(r) {
            v.fill(0);
        }
        Ok(())
    }
}

/// Platforms without an OS backend: nothing can be stored.
pub struct UnavailableStore;

impl SecretStore for UnavailableStore {
    fn put(&self, _: &Secret) -> Result<SecretRef> {
        Err(SecretError::Unavailable)
    }
    fn get(&self, _: &SecretRef) -> Result<Secret> {
        Err(SecretError::Unavailable)
    }
    fn delete(&self, _: &SecretRef) -> Result<()> {
        Ok(())
    }
    fn available(&self) -> bool {
        false
    }
}

/// The OS store for this platform rooted at `dir` (the app's private data
/// folder), or [`UnavailableStore`].
pub fn os_store(dir: &std::path::Path) -> Box<dyn SecretStore> {
    #[cfg(windows)]
    {
        Box::new(DpapiStore::new(dir.to_path_buf()))
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Box::new(UnavailableStore)
    }
}

#[cfg(windows)]
pub use dpapi::DpapiStore;

#[cfg(windows)]
mod dpapi {
    use super::*;
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    /// Optional entropy binding blobs to this app: another program running
    /// as the same user must at least know it to call DPAPI successfully.
    const ENTROPY: &[u8] = b"VideoVault secret store v1";

    pub struct DpapiStore {
        dir: PathBuf,
    }

    impl DpapiStore {
        pub fn new(dir: PathBuf) -> Self {
            DpapiStore { dir }
        }

        fn path(&self, r: &SecretRef) -> PathBuf {
            self.dir.join(format!("{}.vvsec", r.file_stem()))
        }
    }

    fn blob(b: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: b.len() as u32,
            pbData: b.as_ptr() as *mut u8,
        }
    }

    fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        unsafe {
            std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
            LocalFree(out.pbData as _);
        }
        v
    }

    pub(crate) fn protect(plain: &[u8]) -> Result<Vec<u8>> {
        let input = blob(plain);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(SecretError::Os(unsafe { GetLastError() } as i32));
        }
        Ok(take(out))
    }

    pub(crate) fn unprotect(sealed: &[u8]) -> Result<Vec<u8>> {
        let input = blob(sealed);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(SecretError::Os(unsafe { GetLastError() } as i32));
        }
        Ok(take(out))
    }

    impl SecretStore for DpapiStore {
        fn put(&self, secret: &Secret) -> Result<SecretRef> {
            if secret.len() > MAX_SECRET_BYTES {
                return Err(SecretError::TooLarge);
            }
            let sealed = protect(secret.expose())?;
            std::fs::create_dir_all(&self.dir).map_err(SecretError::Io)?;
            let r = SecretRef::generate();
            let path = self.path(&r);
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, &sealed).map_err(SecretError::Io)?;
            std::fs::rename(&tmp, &path).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                SecretError::Io(e)
            })?;
            Ok(r)
        }

        fn get(&self, r: &SecretRef) -> Result<Secret> {
            let sealed = match std::fs::read(self.path(r)) {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(SecretError::NotFound)
                }
                Err(e) => return Err(SecretError::Io(e)),
            };
            unprotect(&sealed).map(Secret::new)
        }

        fn delete(&self, r: &SecretRef) -> Result<()> {
            match std::fs::remove_file(self.path(r)) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(SecretError::Io(e)),
            }
        }
    }
}
