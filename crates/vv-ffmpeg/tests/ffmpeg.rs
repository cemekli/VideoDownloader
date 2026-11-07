//! FFmpeg adapter (STATE.md F0.8c, ADR-0006): fixed flags against
//! `args.json`, ffprobe output of the playback matrix, and the AC-59
//! rejections recorded by `scripts/ffmpeg-probe.mjs`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use vv_domain::error::ErrorCode;
use vv_ffmpeg::args::{self, ArgsError};
use vv_ffmpeg::probe::rejected_demuxer;
use vv_ffmpeg::*;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn json(rel: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(repo().join(rel)).unwrap()).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn os(v: Vec<std::ffi::OsString>) -> Vec<String> {
    v.into_iter().map(|s| s.into_string().unwrap()).collect()
}

#[test]
fn fixed_args_match_the_probe_script() {
    let a = json("tests/fixtures/ffmpeg/args.json");
    assert_eq!(a["formatWhitelist"], args::FORMAT_WHITELIST);
    assert_eq!(strings(&a["common"]), args::COMMON);
    assert_eq!(strings(&a["input"]), args::INPUT);
    assert_eq!(strings(&a["probe"]), args::PROBE);
    assert_eq!(strings(&a["remuxOutput"]), args::REMUX_OUTPUT);
    // The build the fixtures are checked against is the one the manifest pins.
    let m = json("tools/manifest.json");
    let ff = m["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "ffmpeg")
        .unwrap();
    assert_eq!(a["ffmpegBuild"], ff["build"]);
}

#[test]
fn whitelist_has_no_demuxer_that_follows_references() {
    for name in args::FORMAT_WHITELIST.split(',') {
        assert!(!probe::REFERENCING_DEMUXERS.contains(&name), "{name}");
    }
    for name in ["mov,mp4,m4a,3gp,3g2,mj2", "matroska,webm"] {
        assert!(Container::from_format_name(name).is_some());
        assert!(name
            .split(',')
            .all(|n| args::FORMAT_WHITELIST.split(',').any(|w| w == n)));
    }
}

#[test]
fn argv_keeps_paths_whole_and_absolute() {
    let input = if cfg!(windows) {
        PathBuf::from(r"C:\Users\a\Videos\concat:x|http:y.mkv")
    } else {
        PathBuf::from("/home/a/Videos/concat:x|http:y.mkv")
    };
    let out = input.with_file_name("out.mp4");
    let p = os(args::probe_args(&input).unwrap());
    let n = p.len();
    assert_eq!(p[n - 2], "-i");
    assert_eq!(p[n - 1], format!("file:{}", input.display()));
    assert_eq!(&p[3..7], args::INPUT);

    let r = os(args::remux_args(&input, &out).unwrap());
    assert_eq!(r.last().unwrap(), &format!("file:{}", out.display()));
    assert!(r.contains(&"-nostdin".to_owned()));
    let i = r.iter().position(|a| a == "-i").unwrap();
    assert_eq!(
        &r[i - 4..i],
        args::INPUT,
        "whitelists come before the input"
    );

    assert_eq!(
        args::probe_args(Path::new("video.mkv")),
        Err(ArgsError::NotAbsolute)
    );
    assert_eq!(
        args::remux_args(&input, Path::new("out.mp4")),
        Err(ArgsError::NotAbsolute)
    );
}

#[test]
fn playback_matrix_probes_classify() {
    let media = json("tests/fixtures/media/manifest.json");
    for f in media["files"].as_array().unwrap() {
        let id = f["id"].as_str().unwrap();
        let stdout =
            std::fs::read_to_string(repo().join(format!("tests/fixtures/ffmpeg/probe/{id}.json")))
                .unwrap();
        let p = classify(true, &stdout, "").unwrap_or_else(|e| panic!("{id}: {e:?}"));
        let want = match f["container"].as_str().unwrap() {
            "mp4" | "mov" => Container::Mp4,
            "mkv" | "webm" => Container::Matroska,
            other => panic!("{other}"),
        };
        assert_eq!(p.container, want, "{id}");
        assert_eq!(p.video.as_deref(), f["video"].as_str(), "{id}");
        assert_eq!(p.audio.as_deref(), f["audio"].as_str(), "{id}");
        assert_eq!(p.size_bytes, f["bytes"].as_u64(), "{id}");
        let d = p.duration_s.unwrap();
        assert!(
            (d - f["durationS"].as_f64().unwrap()).abs() < 0.1,
            "{id} {d}"
        );
    }
}

#[test]
fn referencing_inputs_are_rejected() {
    let r = json("tests/fixtures/ffmpeg/rejections.json");
    let mut restricted = 0;
    for c in r["cases"].as_array().unwrap() {
        let name = c["case"].as_str().unwrap();
        assert_eq!(
            c["httpRequests"].as_u64(),
            Some(if name == "url-input-unrestricted" {
                1
            } else {
                0
            }),
            "{name}"
        );
        if c["restricted"] != true {
            continue;
        }
        restricted += 1;
        let stderr = strings(&c["stderr"]).join("\n");
        let e = classify(c["exitOk"] == true, "", &stderr).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InputRejected, "{name}: {e:?}");
    }
    assert_eq!(restricted, 4);
    // The control: with only the protocol whitelist, ffprobe opened the HLS
    // playlist (and the file outside the import folder it names).
    let control = r["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case"] == "hls-protocol-whitelist-only")
        .unwrap();
    assert_eq!(control["exitOk"], true);
    assert_eq!(control["formatName"], "hls");
    // Had such output reached classify, it is still refused.
    let fake = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"h264"}],"format":{"format_name":"hls"}}"#;
    assert_eq!(
        classify(true, fake, "").unwrap_err().code(),
        ErrorCode::InputRejected
    );
}

#[test]
fn other_failures_are_media_corrupt() {
    let avi = "[avi @ 0x1] Format not on whitelist 'mov,mp4'\nfile:/x.avi: Invalid argument";
    assert_eq!(rejected_demuxer(avi), Some("avi"));
    let e = classify(false, "", avi).unwrap_err();
    assert_eq!(
        e,
        ProbeError::UnsupportedFormat {
            format: "avi".into()
        }
    );
    assert_eq!(e.code(), ErrorCode::MediaCorrupt);

    let broken = "file:/x.mp4: Invalid data found when processing input";
    assert_eq!(classify(false, "", broken), Err(ProbeError::Unreadable));
    assert_eq!(classify(true, "not json", ""), Err(ProbeError::Unreadable));

    let no_streams =
        r#"{"streams":[{"index":0,"codec_type":"data"}],"format":{"format_name":"matroska,webm"}}"#;
    assert_eq!(
        classify(true, no_streams, ""),
        Err(ProbeError::NoMediaStreams)
    );
    let other = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"mpeg4"}],"format":{"format_name":"avi"}}"#;
    assert_eq!(
        classify(true, other, "").unwrap_err().code(),
        ErrorCode::MediaCorrupt
    );
    // Only the error line names the demuxer; ordinary log lines do not.
    assert_eq!(
        rejected_demuxer("[mov,mp4 @ 0x2] moov atom not found"),
        None
    );
}
