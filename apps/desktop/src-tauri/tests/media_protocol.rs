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

/// The shipped handler (F2.4): an asset id resolved through the library.
#[test]
fn library_assets_are_served_by_id_and_nothing_else() {
    use vv_desktop::library::LibraryState;
    use vv_desktop::media_protocol::dispatch;
    use vv_persistence::library::repo::{self, LibraryItem, MediaAsset, StorageRoot};
    use vv_persistence::library::*;

    let d = tempfile::tempdir().unwrap();
    let videos = d.path().join("videos");
    std::fs::create_dir_all(&videos).unwrap();
    std::fs::copy(media_dir().join(FILE), videos.join("clip.mp4")).unwrap();
    std::fs::write(d.path().join("secret.txt"), b"secret").unwrap();

    let st = LibraryState::default();
    st.open(&d.path().join("data"), None).unwrap();
    let lib = st.get().unwrap();
    let (asset, thumb, missing_root) = lib
        .write(|c| {
            let it = LibraryItem::new(ItemKind::LocalVideo, SourcePlatform::Local, 1);
            repo::insert_item(c, &it).unwrap();
            let root = |path: &Path, status| {
                let r = StorageRoot {
                    id: new_id(),
                    kind: StorageRootKind::ExternalImport,
                    label: "v".into(),
                    path: path.to_string_lossy().into_owned(),
                    volume_id: None,
                    status,
                    created_at: 1,
                    last_verified_at: None,
                };
                repo::insert_storage_root(c, &r).unwrap();
                r
            };
            let ok = root(&videos, StorageRootStatus::Available);
            let away = root(&d.path().join("unplugged"), StorageRootStatus::Disconnected);
            let a = MediaAsset::new(
                &it.id,
                AssetRole::Original,
                MediaKind::Video,
                StorageKind::ExternalReference,
                &ok.id,
                "clip.mp4",
                1,
            );
            repo::insert_media_asset(c, &a).unwrap();
            let escape = MediaAsset::new(
                &it.id,
                AssetRole::Original,
                MediaKind::Video,
                StorageKind::ExternalReference,
                &ok.id,
                "x/../../secret.txt",
                1,
            );
            repo::insert_media_asset(c, &escape).unwrap();
            let mut t = MediaAsset::new(
                &it.id,
                AssetRole::Thumbnail,
                MediaKind::Image,
                StorageKind::Managed,
                &ok.id,
                "clip.mp4",
                1,
            );
            t.derivation = Some("{}".into());
            t.source_asset_id = Some(a.id.clone());
            repo::insert_media_asset(c, &t).unwrap();
            let gone = MediaAsset::new(
                &it.id,
                AssetRole::Original,
                MediaKind::Video,
                StorageKind::ExternalReference,
                &away.id,
                "clip.mp4",
                1,
            );
            repo::insert_media_asset(c, &gone).unwrap();
            Ok((a.id, (t.id, escape.id), gone.id))
        })
        .unwrap();

    let res = dispatch(&st, None, &get(&format!("/{asset}"), Some("bytes=0-99")));
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(header_of(&res, header::CONTENT_TYPE), "video/mp4");
    assert_eq!(res.body().len(), 100);

    // Not playable media, a path leaving the root, or a disconnected root.
    assert_eq!(
        dispatch(&st, None, &get(&format!("/{}", thumb.0), None)).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        dispatch(&st, None, &get(&format!("/{}", thumb.1), None)).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        dispatch(&st, None, &get(&format!("/{missing_root}"), None)).status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    // File names are never resolved without the E2E folder.
    assert_eq!(
        dispatch(&st, None, &get("/clip.mp4", None)).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        dispatch(&st, None, &get("/../secret.txt", None)).status(),
        StatusCode::NOT_FOUND
    );
    // With it, names go there and ids still go to the library.
    let e2e = media_dir();
    assert_eq!(
        dispatch(&st, Some(&e2e), &get(&format!("/{FILE}"), None)).status(),
        StatusCode::OK
    );
    assert_eq!(
        dispatch(
            &st,
            Some(&e2e),
            &get(&format!("/{asset}"), Some("bytes=0-9"))
        )
        .status(),
        StatusCode::PARTIAL_CONTENT
    );
}
