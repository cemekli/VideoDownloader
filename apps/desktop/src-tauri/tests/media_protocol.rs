//! Local media protocol prototype (ADR-0004): range answers and path confinement.
//! The same handler serves the playback matrix in the Windows E2E build.

use std::path::{Path, PathBuf};

use tauri::http::{header, Method, Request, StatusCode};
use vv_desktop::media_protocol::{
    content_type, handle, is_safe_name, parse_range, ByteRange, RangeError, MAX_RANGE_BYTES,
};

fn media_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/media")
        .canonicalize()
        .expect("tests/fixtures/media exists (scripts/make-playback-media.mjs)")
}

fn get(path: &str, range: Option<&str>) -> Request<Vec<u8>> {
    let mut b = Request::builder()
        .method(Method::GET)
        .uri(format!("http://vvmedia.localhost{path}"));
    if let Some(r) = range {
        b = b.header(header::RANGE, r);
    }
    b.body(Vec::new()).unwrap()
}

fn header_of<T>(res: &tauri::http::Response<T>, name: header::HeaderName) -> String {
    res.headers()
        .get(name)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default()
}

const FILE: &str = "mp4-h264-aac.mp4";

#[test]
fn parse_range_forms() {
    let r = |s, e| Ok(ByteRange { start: s, end: e });
    assert_eq!(parse_range("bytes=0-", 1000), r(0, 999));
    assert_eq!(parse_range("bytes=10-19", 1000), r(10, 19));
    assert_eq!(parse_range("bytes=990-5000", 1000), r(990, 999));
    assert_eq!(parse_range("bytes=-100", 1000), r(900, 999));
    assert_eq!(parse_range("bytes=-5000", 1000), r(0, 999));
    assert_eq!(
        parse_range("bytes=1000-", 1000),
        Err(RangeError::Unsatisfiable)
    );
    assert_eq!(
        parse_range("bytes=-0", 1000),
        Err(RangeError::Unsatisfiable)
    );
    assert_eq!(
        parse_range("bytes=0-1,5-9", 1000),
        Err(RangeError::Unsupported)
    );
    assert_eq!(parse_range("items=0-1", 1000), Err(RangeError::Unsupported));
    assert_eq!(parse_range("bytes=9-1", 1000), Err(RangeError::Unsupported));
    assert_eq!(parse_range("bytes=a-", 1000), Err(RangeError::Unsupported));
    assert_eq!(
        parse_range("bytes=+1-2", 1000),
        Err(RangeError::Unsupported)
    );
    assert_eq!(parse_range("bytes=0-", 0), Err(RangeError::Unsatisfiable));
}

#[test]
fn open_ended_ranges_are_capped() {
    let len = 3 * MAX_RANGE_BYTES;
    assert_eq!(
        parse_range("bytes=5-", len),
        Ok(ByteRange {
            start: 5,
            end: 5 + MAX_RANGE_BYTES - 1
        })
    );
}

#[test]
fn range_request_returns_exact_bytes() {
    let root = media_dir();
    let bytes = std::fs::read(root.join(FILE)).unwrap();
    let len = bytes.len();

    let res = handle(&root, &get(&format!("/{FILE}"), Some("bytes=0-")));
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        header_of(&res, header::CONTENT_RANGE),
        format!("bytes 0-{}/{len}", len - 1)
    );
    assert_eq!(header_of(&res, header::CONTENT_TYPE), "video/mp4");
    assert_eq!(header_of(&res, header::ACCEPT_RANGES), "bytes");
    assert_eq!(header_of(&res, header::CONTENT_LENGTH), len.to_string());
    assert_eq!(res.body().as_ref(), &bytes[..]);

    let res = handle(&root, &get(&format!("/{FILE}"), Some("bytes=100-199")));
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.body().as_ref(), &bytes[100..200]);

    let res = handle(&root, &get(&format!("/{FILE}"), Some("bytes=-16")));
    assert_eq!(res.body().as_ref(), &bytes[len - 16..]);
}

#[test]
fn whole_file_without_range_and_head_has_no_body() {
    let root = media_dir();
    let len = std::fs::metadata(root.join(FILE)).unwrap().len();

    let res = handle(&root, &get(&format!("/{FILE}"), None));
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.body().len() as u64, len);

    let head = Request::builder()
        .method(Method::HEAD)
        .uri(format!("http://vvmedia.localhost/{FILE}"))
        .body(Vec::new())
        .unwrap();
    let res = handle(&root, &head);
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(header_of(&res, header::CONTENT_LENGTH), len.to_string());
    assert!(res.body().is_empty());
}

#[test]
fn bad_ranges_get_416_with_length() {
    let root = media_dir();
    let len = std::fs::metadata(root.join(FILE)).unwrap().len();
    for r in [
        format!("bytes={len}-"),
        "bytes=0-1,4-5".into(),
        "lines=1-2".into(),
    ] {
        let res = handle(&root, &get(&format!("/{FILE}"), Some(&r)));
        assert_eq!(res.status(), StatusCode::RANGE_NOT_SATISFIABLE, "{r}");
        assert_eq!(
            header_of(&res, header::CONTENT_RANGE),
            format!("bytes */{len}")
        );
    }
}

#[test]
fn only_get_and_head() {
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("http://vvmedia.localhost/{FILE}"))
        .body(Vec::new())
        .unwrap();
    assert_eq!(
        handle(&media_dir(), &req).status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[test]
fn nothing_outside_the_served_directory() {
    let root = media_dir();
    for path in [
        "/",
        "/../Cargo.toml",
        "/..%2fCargo.toml",
        "/%2e%2e/Cargo.toml",
        "/..\\Cargo.toml",
        "/./manifest.json",
        "/sub/x.mp4",
        "/.hidden",
        "/C:/Windows/win.ini",
        "//etc/passwd",
        "/missing.mp4",
    ] {
        let res = handle(&root, &get(path, Some("bytes=0-")));
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
        assert!(res.body().is_empty(), "{path}");
    }
}

#[cfg(unix)]
#[test]
fn symlink_out_of_the_directory_is_refused() {
    let dir = std::env::temp_dir().join(format!("vv-media-proto-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let link = dir.join("escape.mp4");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
        &link,
    )
    .unwrap();
    let root = dir.canonicalize().unwrap();
    let res = handle(&root, &get("/escape.mp4", Some("bytes=0-")));
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[test]
fn names_and_types() {
    assert!(is_safe_name("mkv-av1-opus.mkv"));
    assert!(!is_safe_name(""));
    assert!(!is_safe_name(".x"));
    assert!(!is_safe_name("a/b"));
    assert!(!is_safe_name("a..b"));
    assert!(!is_safe_name("a%2e"));
    assert!(!is_safe_name(&"a".repeat(129)));
    assert_eq!(content_type("x.MKV"), "video/x-matroska");
    assert_eq!(content_type("x.webm"), "video/webm");
    assert_eq!(content_type("x.mov"), "video/quicktime");
    assert_eq!(content_type("x.exe"), "application/octet-stream");
}
