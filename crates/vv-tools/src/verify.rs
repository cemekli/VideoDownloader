//! File verification against the manifest (SPEC §23.1, AC-25).
//!
//! A file that is missing, the wrong size or the wrong hash is never run. Size
//! is checked first so a truncated or swapped file fails without hashing.

use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use vv_domain::error::ErrorCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// Not there, or not a regular file (antivirus quarantine looks like this).
    Missing,
    /// Could not be read; reported like a missing tool.
    Unreadable(String),
    SizeMismatch {
        expected: u64,
        actual: u64,
    },
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
}

impl VerifyError {
    /// SPEC §19 code the UI shows.
    pub fn code(&self) -> ErrorCode {
        match self {
            VerifyError::Missing | VerifyError::Unreadable(_) => ErrorCode::ToolMissing,
            VerifyError::SizeMismatch { .. } | VerifyError::ChecksumMismatch { .. } => {
                ErrorCode::ChecksumMismatch
            }
        }
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::Missing => write!(f, "file is missing"),
            VerifyError::Unreadable(e) => write!(f, "file cannot be read: {e}"),
            VerifyError::SizeMismatch { expected, actual } => {
                write!(f, "size {actual} bytes, expected {expected}")
            }
            VerifyError::ChecksumMismatch { expected, actual } => {
                write!(f, "SHA-256 {actual}, expected {expected}")
            }
        }
    }
}

impl std::error::Error for VerifyError {}

/// Lowercase hex SHA-256 of a file, read in 64 KiB blocks.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut reader = BufReader::with_capacity(64 * 1024, File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Checks a file against an expected size (when known) and SHA-256.
pub fn verify_file(
    path: &Path,
    expected_sha256: &str,
    expected_size: Option<u64>,
) -> Result<(), VerifyError> {
    let meta = match path.metadata() {
        Ok(m) if m.is_file() => m,
        Ok(_) => return Err(VerifyError::Missing),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(VerifyError::Missing),
        Err(e) => return Err(VerifyError::Unreadable(e.kind().to_string())),
    };
    if let Some(expected) = expected_size {
        if meta.len() != expected {
            return Err(VerifyError::SizeMismatch {
                expected,
                actual: meta.len(),
            });
        }
    }
    let actual = sha256_file(path).map_err(|e| VerifyError::Unreadable(e.kind().to_string()))?;
    if actual != expected_sha256 {
        return Err(VerifyError::ChecksumMismatch {
            expected: expected_sha256.to_owned(),
            actual,
        });
    }
    Ok(())
}
