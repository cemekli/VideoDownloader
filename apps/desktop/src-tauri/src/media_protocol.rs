//! Local media protocol prototype (SPEC DEC-04, STATE.md F0.8a, ADR-0004).
//!
//! Serves files from one directory over a custom URI scheme with HTTP range
//! support, so `<video>` in WebView2 can seek. The playback matrix is measured
//! through this handler because WebView2's behaviour depends on the protocol
//! (MIME type, range answers), not only on the codec.
//!
//! Shipped since F2.4 (ADR-0016): a request names a `MediaAsset` id, which is
//! resolved through the database to an original or preview video/audio file
//! inside an available storage root, never a path chosen by the webview. The
//! E2E test build (feature `e2e-cdp`) also serves plain file names from
//! `VV_E2E_MEDIA_DIR` for the playback matrix.

use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use tauri::http::{header, Method, Request, Response, StatusCode};

/// `http://vvmedia.localhost/<name>` on Windows, `vvmedia://localhost/<name>` elsewhere.
pub const SCHEME: &str = "vvmedia";

/// Largest body for one range answer. The player asks again for the rest.
pub const MAX_RANGE_BYTES: u64 = 4 * 1024 * 1024;

/// Largest file sent whole when a request has no `Range` header.
pub const MAX_WHOLE_BYTES: u64 = 64 * 1024 * 1024;

/// A byte range, both ends inclusive, already clamped to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeError {
    /// Syntax we do not accept (other units, several ranges, garbage).
    Unsupported,
    /// Well-formed but outside the file.
    Unsatisfiable,
}

/// Parses a single `bytes=` range (RFC 9110 §14.1.2) against a file of `len`
/// bytes. Several ranges are refused: media players never send them.
pub fn parse_range(value: &str, len: u64) -> Result<ByteRange, RangeError> {
    let spec = value
        .trim()
        .strip_prefix("bytes=")
        .ok_or(RangeError::Unsupported)?;
    if spec.contains(',') {
        return Err(RangeError::Unsupported);
    }
    let (first, last) = spec.split_once('-').ok_or(RangeError::Unsupported)?;
    let num = |s: &str| -> Result<u64, RangeError> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(RangeError::Unsupported);
        }
        s.parse().map_err(|_| RangeError::Unsupported)
    };
    let range = match (first.trim(), last.trim()) {
        ("", suffix) => {
            let n = num(suffix)?;
            if n == 0 || len == 0 {
                return Err(RangeError::Unsatisfiable);
            }
            ByteRange {
                start: len.saturating_sub(n),
                end: len - 1,
            }
        }
        (start, "") => {
            let start = num(start)?;
            if start >= len {
                return Err(RangeError::Unsatisfiable);
            }
            ByteRange {
                start,
                end: len - 1,
            }
        }
        (start, end) => {
            let (start, end) = (num(start)?, num(end)?);
            if start > end {
                return Err(RangeError::Unsupported);
            }
            if start >= len {
                return Err(RangeError::Unsatisfiable);
            }
            ByteRange {
                start,
                end: end.min(len - 1),
            }
        }
    };
    // Open-ended answers are cut; the player continues with the next range.
    Ok(ByteRange {
        start: range.start,
        end: range.end.min(range.start + MAX_RANGE_BYTES - 1),
    })
}

/// Only plain names inside the served directory: no separators, no `..`, no
/// percent-encoding (which could hide either), nothing hidden.
pub fn is_safe_name(name: &str) -> bool {
    let first_ok = name
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric());
    first_ok
        && name.len() <= 128
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

pub fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) {
        Some(e) if e == "mp4" || e == "m4v" => "video/mp4",
        Some(e) if e == "webm" => "video/webm",
        Some(e) if e == "mkv" => "video/x-matroska",
        Some(e) if e == "mov" => "video/quicktime",
        Some(e) if e == "m4a" => "audio/mp4",
        Some(e) if e == "opus" || e == "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

/// Resolves a request path to a regular file under `root` (canonicalized).
fn resolve(root: &Path, uri_path: &str) -> Option<PathBuf> {
    let name = uri_path.strip_prefix('/')?;
    if !is_safe_name(name) {
        return None;
    }
    let path = root.join(name).canonicalize().ok()?;
    (path.starts_with(root) && path.is_file()).then_some(path)
}

type Body = Cow<'static, [u8]>;

fn plain(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Cow::Borrowed(&[][..]))
        .expect("static response")
}

fn read_range(path: &Path, range: ByteRange) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(range.start))?;
    let mut buf = Vec::with_capacity((range.end - range.start + 1) as usize);
    file.take(range.end - range.start + 1)
        .read_to_end(&mut buf)?;
    Ok(buf)
}

/// Answers one protocol request for a file name under `root` (E2E matrix).
/// `root` must already be canonicalized.
pub fn handle(root: &Path, request: &Request<Vec<u8>>) -> Response<Body> {
    if let Some(r) = refuse_method(request) {
        return r;
    }
    let Some(path) = resolve(root, request.uri().path()) else {
        return plain(StatusCode::NOT_FOUND);
    };
    serve(&path, request)
}

fn refuse_method(request: &Request<Vec<u8>>) -> Option<Response<Body>> {
    (request.method() != Method::GET && request.method() != Method::HEAD)
        .then(|| plain(StatusCode::METHOD_NOT_ALLOWED))
}

/// The file of a playable asset, when its root is available and the file
/// resolves inside the root (a planted link or junction is refused).
pub fn asset_file(root: &Path, storage_ref: &str) -> Option<PathBuf> {
    let root = std::fs::canonicalize(root).ok()?;
    let file = std::fs::canonicalize(vv_import::fsutil::resolve_ref(&root, storage_ref)).ok()?;
    (file.starts_with(&root) && file.is_file()).then_some(file)
}

/// The shipped handler: `/<asset-id>`, resolved through the database.
pub fn handle_asset(
    state: &crate::library::LibraryState,
    request: &Request<Vec<u8>>,
) -> Response<Body> {
    if let Some(r) = refuse_method(request) {
        return r;
    }
    let id = request.uri().path().trim_start_matches('/');
    if !crate::thumb_protocol::valid_id(id) {
        return plain(StatusCode::NOT_FOUND);
    }
    let loc = state
        .get()
        .and_then(|lib| lib.reader())
        .ok()
        .and_then(|c| vv_persistence::library::view::media_location(&c, id).ok())
        .flatten();
    let Some((root, storage_ref, status)) = loc else {
        return plain(StatusCode::NOT_FOUND);
    };
    if status != vv_persistence::library::StorageRootStatus::Available {
        return plain(StatusCode::SERVICE_UNAVAILABLE);
    }
    match asset_file(Path::new(&root), &storage_ref) {
        Some(path) => serve(&path, request),
        None => plain(StatusCode::NOT_FOUND),
    }
}

/// Asset ids go to the library; other names only to the E2E matrix folder.
pub fn dispatch(
    state: &crate::library::LibraryState,
    e2e_root: Option<&Path>,
    request: &Request<Vec<u8>>,
) -> Response<Body> {
    let name = request.uri().path().trim_start_matches('/');
    match e2e_root {
        Some(root) if !crate::thumb_protocol::valid_id(name) => handle(root, request),
        _ => handle_asset(state, request),
    }
}

/// `http://vvmedia.localhost/<id>` on Windows, `vvmedia://localhost/<id>` elsewhere.
pub fn media_url(asset_id: &str) -> String {
    if cfg!(windows) {
        format!("http://{SCHEME}.localhost/{asset_id}")
    } else {
        format!("{SCHEME}://localhost/{asset_id}")
    }
}

/// Range-serves one resolved file.
fn serve(path: &Path, request: &Request<Vec<u8>>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = path.to_path_buf();
    let Ok(len) = path.metadata().map(|m| m.len()) else {
        return plain(StatusCode::NOT_FOUND);
    };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let not_satisfiable = || {
        let mut res = plain(StatusCode::RANGE_NOT_SATISFIABLE);
        let value = format!("bytes */{len}").parse().expect("ascii header");
        res.headers_mut().insert(header::CONTENT_RANGE, value);
        res
    };
    let range = match request.headers().get(header::RANGE) {
        None if len <= MAX_WHOLE_BYTES => None,
        // Too big to send whole; a media element always asks for ranges.
        None => return not_satisfiable(),
        Some(value) => match value.to_str() {
            Ok(v) => match parse_range(v, len) {
                Ok(r) => Some(r),
                Err(_) => return not_satisfiable(),
            },
            Err(_) => return not_satisfiable(),
        },
    };

    let (status, span) = match range {
        Some(r) => (StatusCode::PARTIAL_CONTENT, r),
        None if len == 0 => (StatusCode::OK, ByteRange { start: 0, end: 0 }),
        None => (
            StatusCode::OK,
            ByteRange {
                start: 0,
                end: len - 1,
            },
        ),
    };
    let body_len = if len == 0 {
        0
    } else {
        span.end - span.start + 1
    };
    let body = if head || body_len == 0 {
        Vec::new()
    } else {
        match read_range(&path, span) {
            Ok(b) => b,
            Err(_) => return plain(StatusCode::INTERNAL_SERVER_ERROR),
        }
    };

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type(name))
        .header(header::CONTENT_LENGTH, body_len.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header("X-Content-Type-Options", "nosniff")
        .header(header::CACHE_CONTROL, "no-store");
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{len}", span.start, span.end),
        );
    }
    builder.body(Cow::Owned(body)).expect("valid response")
}
