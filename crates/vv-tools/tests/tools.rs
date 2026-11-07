//! Managed tools (STATE.md F0.8b, ADR-0005): the real manifest, tampered
//! manifests, file verification and version checks against real tool output.

use serde_json::Value;
use std::path::{Path, PathBuf};
use vv_domain::error::ErrorCode;
use vv_tools::health::{check_version, VersionCheck};
use vv_tools::manifest::{ManifestError, YTDLP_SIGNING_KEY};
use vv_tools::verify::sha256_file;
use vv_tools::*;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest_text() -> String {
    std::fs::read_to_string(repo().join("tools/manifest.json")).unwrap()
}

fn health() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(repo().join("tests/fixtures/tools/health-output.json")).unwrap(),
    )
    .unwrap()
}

/// The manifest with one JSON edit applied.
fn tampered(edit: impl FnOnce(&mut Value)) -> Result<Manifest, ManifestError> {
    let mut v: Value = serde_json::from_str(&manifest_text()).unwrap();
    edit(&mut v);
    Manifest::parse(&v.to_string())
}

#[test]
fn shipped_manifest_is_valid_and_complete() {
    let m = Manifest::parse(&manifest_text()).expect("tools/manifest.json is valid");
    for id in [ToolId::YtDlp, ToolId::Deno] {
        for platform in ["windows-x86_64", "linux-x86_64"] {
            assert!(
                m.artifact(id, platform).is_some(),
                "{} {platform}",
                id.as_str()
            );
        }
    }
    let ytdlp = m.tool(ToolId::YtDlp).unwrap();
    // One pinned yt-dlp for the whole app: the fixtures were captured with it.
    assert_eq!(ytdlp.version, vv_ytdlp::args::PINNED_VERSION);
    assert!(
        matches!(&ytdlp.provenance, Provenance::GpgSignedSums { key_fingerprint, .. } if key_fingerprint == YTDLP_SIGNING_KEY)
    );
    // The pinned Deno is new enough for the pinned yt-dlp.
    let deno = Version::parse(&m.tool(ToolId::Deno).unwrap().version).unwrap();
    assert!(deno >= DENO_MIN_FOR_YTDLP);
}

type Edit = Box<dyn FnOnce(&mut Value)>;

#[test]
fn manifest_cannot_point_elsewhere() {
    let cases: Vec<(&str, Edit)> = vec![
        (
            "other host",
            Box::new(|v| {
                v["tools"][0]["artifacts"][0]["url"] =
                    "https://evil.example/yt-dlp/yt-dlp/releases/download/2026.10.26/yt-dlp.exe"
                        .into()
            }),
        ),
        (
            "other repo",
            Box::new(|v| {
                v["tools"][0]["artifacts"][0]["url"] =
                    "https://github.com/someone/yt-dlp/releases/download/2026.10.26/yt-dlp.exe"
                        .into()
            }),
        ),
        (
            "plain http",
            Box::new(|v| {
                v["tools"][1]["artifacts"][0]["url"] = "http://github.com/denoland/deno/releases/download/v2.9.8/deno-x86_64-pc-windows-msvc.zip".into()
            }),
        ),
        (
            "latest link",
            Box::new(|v| {
                v["tools"][1]["artifacts"][0]["url"] =
                    "https://github.com/denoland/deno/releases/download/latest/deno.zip".into()
            }),
        ),
        (
            "version not in URL",
            Box::new(|v| v["tools"][0]["version"] = "2026.11.01".into()),
        ),
        (
            "query string",
            Box::new(|v| {
                v["tools"][0]["artifacts"][0]["url"] =
                    "https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.26/yt-dlp.exe?x=1"
                        .into()
            }),
        ),
        (
            "dot-dot",
            Box::new(|v| {
                v["tools"][0]["artifacts"][0]["url"] = "https://github.com/yt-dlp/yt-dlp/releases/download/2026.10.26/../../../../evil/x.exe".into()
            }),
        ),
        (
            "other signing key",
            Box::new(|v| {
                v["tools"][0]["provenance"]["keyFingerprint"] =
                    "0000000000000000000000000000000000000000".into()
            }),
        ),
        (
            "yt-dlp without signature",
            Box::new(|v| {
                v["tools"][0]["provenance"] = serde_json::json!({"kind": "published-sha256"})
            }),
        ),
        (
            "sums elsewhere",
            Box::new(|v| {
                v["tools"][0]["provenance"]["sumsUrl"] = "https://evil.example/SHA2-256SUMS".into()
            }),
        ),
        (
            "short hash",
            Box::new(|v| v["tools"][1]["artifacts"][0]["sha256"] = "abc".into()),
        ),
        (
            "uppercase hash",
            Box::new(|v| {
                v["tools"][1]["artifacts"][0]["sha256"] =
                    "CBA84B26E0DF5AE7F74FC66B86A95B866872A86292EBD84FA1E959927EDD93BF".into()
            }),
        ),
        (
            "bare exe with two hashes",
            Box::new(|v| v["tools"][0]["artifacts"][0]["executableSha256"] = "0".repeat(64).into()),
        ),
        (
            "executable path",
            Box::new(|v| v["tools"][1]["artifacts"][0]["executable"] = "../deno.exe".into()),
        ),
        (
            "unknown archive",
            Box::new(|v| v["tools"][1]["artifacts"][0]["archive"] = "tar.gz".into()),
        ),
        (
            "unknown platform",
            Box::new(|v| v["tools"][1]["artifacts"][0]["platform"] = "windows-arm64".into()),
        ),
        (
            "zero size",
            Box::new(|v| v["tools"][1]["artifacts"][0]["size"] = 0.into()),
        ),
        (
            "empty licence",
            Box::new(|v| v["tools"][1]["license"] = " ".into()),
        ),
        (
            "unknown field",
            Box::new(|v| v["tools"][1]["artifacts"][0]["postInstall"] = "rm -rf".into()),
        ),
        (
            "unknown tool",
            Box::new(|v| v["tools"][1]["id"] = "node".into()),
        ),
        (
            "missing deno",
            Box::new(|v| {
                v["tools"].as_array_mut().unwrap().pop();
            }),
        ),
        (
            "duplicate platform",
            Box::new(|v| {
                let a = v["tools"][1]["artifacts"][0].clone();
                v["tools"][1]["artifacts"].as_array_mut().unwrap().push(a);
            }),
        ),
        ("new schema", Box::new(|v| v["schemaVersion"] = 2.into())),
    ];
    for (name, edit) in cases {
        assert!(tampered(edit).is_err(), "{name} was accepted");
    }
}

#[test]
fn verify_detects_missing_resized_and_swapped_files() {
    let dir = std::env::temp_dir().join(format!("vv-tools-verify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tool.exe");
    std::fs::write(&file, b"VideoVault test tool\n").unwrap();
    let good = sha256_file(&file).unwrap();
    // sha256("VideoVault test tool\n"), computed independently with sha256sum.
    assert_eq!(
        good,
        "70c306af8eadc11669c996790890f3585bd75f9748f7ba72af073550a6880fdc"
    );

    assert_eq!(verify_file(&file, &good, Some(21)), Ok(()));
    assert_eq!(verify_file(&file, &good, None), Ok(()));

    let e = verify_file(&file, &good, Some(22)).unwrap_err();
    assert_eq!(
        e,
        VerifyError::SizeMismatch {
            expected: 22,
            actual: 21
        }
    );
    assert_eq!(e.code(), ErrorCode::ChecksumMismatch);

    std::fs::write(&file, b"VideoVault test toox\n").unwrap();
    let e = verify_file(&file, &good, Some(21)).unwrap_err();
    assert!(matches!(e, VerifyError::ChecksumMismatch { .. }));
    assert_eq!(e.code(), ErrorCode::ChecksumMismatch);

    let e = verify_file(&dir.join("absent.exe"), &good, None).unwrap_err();
    assert_eq!(
        (e.clone(), e.code()),
        (VerifyError::Missing, ErrorCode::ToolMissing)
    );
    // A directory where the tool should be (e.g. a quarantine stub) is missing too.
    assert_eq!(verify_file(&dir, &good, None), Err(VerifyError::Missing));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn known_sha256_vector() {
    let dir = std::env::temp_dir().join(format!("vv-tools-vector-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("abc");
    std::fs::write(&file, b"abc").unwrap();
    // FIPS 180-2 test vector.
    assert_eq!(
        sha256_file(&file).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn versions_from_real_tool_output() {
    let h = health();
    let m = Manifest::parse(&manifest_text()).unwrap();
    let deno = parse_deno_version(h["denoVersion"].as_str().unwrap());
    let ytdlp = parse_ytdlp_version(h["ytDlpVersion"].as_str().unwrap());
    assert_eq!(
        check_version(&m.tool(ToolId::Deno).unwrap().version, deno),
        VersionCheck::Ok
    );
    assert_eq!(
        check_version(&m.tool(ToolId::YtDlp).unwrap().version, ytdlp),
        VersionCheck::Ok
    );

    let old = parse_deno_version(h["oldDenoVersion"].as_str().unwrap()).unwrap();
    assert_eq!(old, Version([1, 46, 3]));
    assert!(old < DENO_MIN_FOR_YTDLP);
    let c = check_version("2.9.8", Some(old));
    assert!(matches!(c, VersionCheck::Mismatch { .. }));
    assert_eq!(c.code(), Some(ErrorCode::ToolMissing));

    for junk in [
        "",
        "Access is denied.",
        "deno",
        "deno x.y.z",
        "node v24.1.0",
        "2026.10.26.235959",
    ] {
        assert_eq!(
            check_version(
                "2.9.8",
                parse_deno_version(junk).or(parse_ytdlp_version(junk))
            )
            .code(),
            Some(ErrorCode::ToolMissing),
            "{junk:?}"
        );
    }
    assert_eq!(
        parse_ytdlp_version("2026.01.05\n"),
        Some(Version([2026, 1, 5]))
    );
}

#[test]
fn js_runtimes_lines_from_real_runs() {
    for case in health()["jsRuntimes"].as_array().unwrap() {
        let line = case["line"].as_str().unwrap();
        let parsed = parse_js_runtimes_line(line).unwrap_or_else(|| panic!("unparsed: {line}"));
        let kind = match &parsed {
            JsRuntimeLine::Disabled => "disabled",
            JsRuntimeLine::NotFound => "not_found",
            JsRuntimeLine::Supported { .. } => "supported",
            JsRuntimeLine::Unsupported { .. } => "unsupported",
        };
        assert_eq!(kind, case["expected"], "{}", case["case"]);
        // The args recorded with each capture are exactly what the adapter builds.
        let args: Vec<String> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_owned())
            .collect();
        if args.first().map(String::as_str) == Some("--no-js-runtimes") && args.len() == 4 {
            let built: Vec<String> = vv_ytdlp::args::js_runtime_args(Some(Path::new("<deno>")))
                .into_iter()
                .map(|a| a.into_string().unwrap())
                .collect();
            assert_eq!(built[0], args[0]);
            assert_eq!(built[3], args[3]);
            assert!(built[2].starts_with("deno:"));
        }
    }
    assert_eq!(
        parse_js_runtimes_line("[debug] JS runtimes: deno-2.9.8"),
        Some(JsRuntimeLine::Supported {
            name: "deno".into(),
            version: "2.9.8".into()
        })
    );
    assert_eq!(parse_js_runtimes_line("[debug] Python 3.14.10"), None);
}

#[test]
fn without_managed_deno_no_runtime_is_enabled() {
    let a: Vec<String> = vv_ytdlp::args::js_runtime_args(None)
        .into_iter()
        .map(|a| a.into_string().unwrap())
        .collect();
    assert_eq!(a, ["--no-js-runtimes", "--no-remote-components"]);
    let a = vv_ytdlp::args::js_runtime_args(Some(Path::new(
        "C:\\Program Files\\VideoVault\\tools\\deno.exe",
    )));
    assert_eq!(a[2], "deno:C:\\Program Files\\VideoVault\\tools\\deno.exe");
    assert_eq!(a.len(), 4, "the path stays one argv entry, never split");
}

#[test]
fn deno_is_started_without_permissions() {
    // Evidence recorded from the pinned yt-dlp and Deno (ADR-0005).
    let s = &health()["denoSandbox"];
    let args: Vec<&str> = s["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert!(
        args.iter().all(|a| !a.starts_with("--allow") && *a != "-A"),
        "{args:?}"
    );
    for (what, result) in s["results"].as_object().unwrap() {
        assert_eq!(result, "NotCapable", "{what}");
    }
}
