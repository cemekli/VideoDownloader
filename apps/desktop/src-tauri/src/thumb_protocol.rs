//! Thumbnail protocol for the Library screen (STATE.md F1.7, SPEC §18.2,
//! ADR-0013).
//!
//! `vvthumb://localhost/<asset-id>` (Windows: `http://vvthumb.localhost/...`)
//! answers with the JPEG of a `MediaAsset(role = thumbnail)` in a cache root
//! and nothing else: the request names an asset id, never a path, and the
//! user's own media is not reachable through it. CSP `img-src` allows only
//! this scheme besides `'self'`.

use crate::library::LibraryState;
use std::borrow::Cow;
use std::io::Read;
use std::path::Path;
use tauri::http::{header, Method, Request, Response, StatusCode};

/// Generated thumbnails are 320 px wide; anything far larger is not ours.
pub const MAX_THUMBNAIL_BYTES: u64 = 4 * 1024 * 1024;

type Body = Cow<'static, [u8]>;

fn status(code: StatusCode) -> Response<Body> {
    Response::builder()
        .status(code)
        .header(header::CONTENT_TYPE, "text/plain")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Cow::Borrowed(&[][..]))
        .expect("static response")
}

/// An asset id as `new_id` makes them: a lowercase hyphenated UUID.
pub fn valid_id(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
        })
}

/// Reads the thumbnail file, refusing anything that resolves outside its
/// cache root (a planted link, a `..` in a tampered row).
pub fn read_inside(root: &Path, storage_ref: &str) -> Option<Vec<u8>> {
    let root = std::fs::canonicalize(root).ok()?;
    let file = std::fs::canonicalize(vv_import::fsutil::resolve_ref(&root, storage_ref)).ok()?;
    if !file.starts_with(&root) || !file.is_file() {
        return None;
    }
    let mut buf = Vec::new();
    std::fs::File::open(&file)
        .ok()?
        .take(MAX_THUMBNAIL_BYTES + 1)
        .read_to_end(&mut buf)
        .ok()?;
    (buf.len() as u64 <= MAX_THUMBNAIL_BYTES).then_some(buf)
}

pub fn handle(state: &LibraryState, request: &Request<Vec<u8>>) -> Response<Body> {
    if request.method() != Method::GET {
        return status(StatusCode::METHOD_NOT_ALLOWED);
    }
    let id = request.uri().path().trim_start_matches('/');
    if !valid_id(id) {
        return status(StatusCode::NOT_FOUND);
    }
    let Some(loc) = state.get().ok().and_then(|lib| {
        // One reader per worker thread, kept between requests: a grid asks
        // for hundreds of thumbnails at once.
        thread_local! {
            static READER: std::cell::RefCell<Option<vv_persistence::rusqlite::Connection>> =
                const { std::cell::RefCell::new(None) };
        }
        READER.with(|r| {
            let mut r = r.borrow_mut();
            if r.is_none() {
                *r = lib.reader().ok();
            }
            let found = vv_persistence::library::view::thumbnail_location(r.as_ref()?, id);
            if found.is_err() {
                *r = None; // a broken connection is opened again next time
            }
            found.ok().flatten()
        })
    }) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Some(bytes) = read_inside(Path::new(&loc.0), &loc.1) else {
        return status(StatusCode::NOT_FOUND);
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CACHE_CONTROL, "max-age=3600")
        .header("X-Content-Type-Options", "nosniff")
        .body(Cow::Owned(bytes))
        .expect("thumbnail response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_must_be_uuids() {
        assert!(valid_id("0b2fb1d0-23a9-4b8e-9c1f-0123456789ab"));
        for bad in [
            "",
            "../../etc/passwd",
            "0B2FB1D0-23A9-4B8E-9C1F-0123456789AB",
            "0b2fb1d0-23a9-4b8e-9c1f-0123456789a",
            "0b2fb1d0x23a9-4b8e-9c1f-0123456789ab",
            "0b2fb1d0-23a9-4b8e-9c1f-0123456789a/",
        ] {
            assert!(!valid_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn reads_only_inside_the_root() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("cache");
        std::fs::create_dir_all(root.join("thumbnails")).unwrap();
        std::fs::write(root.join("thumbnails/t.jpg"), b"jpeg").unwrap();
        std::fs::write(d.path().join("secret.txt"), b"secret").unwrap();
        assert_eq!(
            read_inside(&root, "thumbnails/t.jpg").as_deref(),
            Some(&b"jpeg"[..])
        );
        assert_eq!(read_inside(&root, "../secret.txt"), None);
        assert_eq!(read_inside(&root, "thumbnails/none.jpg"), None);
        assert_eq!(read_inside(&root, "thumbnails"), None);
    }
}
