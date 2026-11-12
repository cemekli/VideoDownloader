//! Self-authored, realistic caption files (`tests/fixtures/captions/`):
//! what yt-dlp writes for a Turkish YouTube track with `&gt;&gt;` speaker
//! marks, a rolling English automatic track, and hand-made edge cases
//! (BOM, CRLF, NFD, hours, broken entities). STATE.md F6.2; the bug of
//! 11 Nov 2026 came from exactly such a Turkish track.

use serde_json::Value;
use std::path::PathBuf;
use vv_ytdlp::captions::parse_vtt;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/captions")
}

#[test]
fn every_caption_file_parses_to_its_expected_cues() {
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(dir().join("expected.json")).unwrap())
            .unwrap();
    let files = expected["files"].as_object().unwrap();
    let mut on_disk: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".vtt"))
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = files.keys().cloned().collect();
    listed.sort();
    assert_eq!(
        on_disk, listed,
        "every .vtt has expected cues and the other way round"
    );
    for (name, cues) in files {
        let got = parse_vtt(&std::fs::read(dir().join(name)).unwrap()).unwrap();
        let got: Vec<Value> = got
            .iter()
            .map(|c| serde_json::json!([c.start_ms, c.end_ms, c.text]))
            .collect();
        assert_eq!(&Value::Array(got), cues, "{name}");
    }
}
