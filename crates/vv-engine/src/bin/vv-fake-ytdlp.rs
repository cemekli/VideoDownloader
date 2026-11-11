//! Test double for yt-dlp (`vv-engine` tests only; never shipped or run by
//! the app). It reads the same argv the engine builds and acts out the
//! scenario named by the URL path, `https://fake.test/<scenario>`, writing
//! the same kinds of lines the real yt-dlp writes (`tests/fixtures/ytdlp/`).
//! The environment is cleared by `vv-process`, so the scenario has to come
//! from the URL; state across attempts lives in a marker file in `-P`.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

const MEDIA: &[u8] = include_bytes!("../../../../tests/fixtures/media/mp4-h264-aac.mp4");
const CANARY: &str = "VVCANARY-7f3a91";
// Self-authored YouTube-like caption files (tests/fixtures/captions/).
const YOUTUBE_TR: &str = include_str!("../../../../tests/fixtures/captions/youtube-tr-manual.vtt");
const YOUTUBE_EN_AUTO: &str =
    include_str!("../../../../tests/fixtures/captions/youtube-en-auto.vtt");

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn progress(done: u64, total: Option<u64>, status: &str) {
    let total = total.map_or("NA".to_owned(), |t| t.to_string());
    println!(
        "vv-progress {{\"status\":\"{status}\",\"downloaded_bytes\":{done},\"total_bytes\":{total},\"total_bytes_estimate\":NA,\"eta\":NA,\"speed\":NA,\"elapsed\":0.1,\"fragment_index\":NA,\"fragment_count\":NA}}"
    );
    let _ = std::io::stdout().flush();
}

fn fail(msg: &str) -> ! {
    eprintln!("ERROR: {msg}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("2026.10.26");
        return;
    }
    let url = args.last().cloned().unwrap_or_default();
    let scenario = url.rsplit('/').next().unwrap_or("").to_owned();
    let dir = PathBuf::from(arg_after(&args, "-P").unwrap_or_else(|| ".".into()));
    let metadata = args.iter().any(|a| a == "--dump-single-json");

    match scenario.as_str() {
        "canary-error" => {
            eprintln!(
                "WARNING: [generic] cookies: Cookie: session={CANARY}; Authorization: Bearer {CANARY}"
            );
            fail(&format!(
                "[generic] Unable to download webpage: HTTP Error 403: Forbidden (caused by <HTTPError 403: 'https://cdn.example.com/v.mp4?sig={CANARY}&token={CANARY}'>) at C:\\Users\\{CANARY}\\x"
            ));
        }
        "notfound" => fail("[generic] Unable to download webpage: HTTP Error 404: Not Found"),
        "diskfull" => fail("unable to write data: [Errno 28] No space left on device"),
        "login" => fail("[vk] This video is only available for registered users"),
        // F5.2: needs `--cookies-from-browser firefox:<folder>`. The folder's
        // name says how the browser store behaves.
        "login-auth" => match arg_after(&args, "--cookies-from-browser") {
            None => fail("[Reddit] 1abc2de: Account authentication is required. Use --cookies-from-browser or --cookies for the authentication."),
            Some(src) if !src.starts_with("firefox:") => fail("usage: bad browser"),
            Some(src) if src.ends_with("broken") => {
                fail("could not find firefox cookies database in /home/someone/.mozilla/firefox/broken")
            }
            Some(src) if src.ends_with("signedout") => fail("[Reddit] 1abc2de: Account authentication is required. Use --cookies-from-browser or --cookies for the authentication."),
            Some(_) => {}
        },
        // F5.3: needs `--cookies <working copy>` holding a Reddit session
        // cookie. Like the real yt-dlp it writes the cookie jar back to that
        // file, and it notes where the file was (`<staging>/fake-cookie-seen`)
        // so the test can check the copy is gone afterwards.
        "login-file" | "login-file-slow" => match arg_after(&args, "--cookies") {
            None => fail("[Reddit] 1abc2de: Account authentication is required. Use --cookies-from-browser or --cookies for the authentication."),
            Some(path) => {
                let path = PathBuf::from(path);
                if let Some(staging) = path.parent().and_then(|d| d.parent()) {
                    let _ = std::fs::write(staging.join("fake-cookie-seen"), path.to_string_lossy().as_bytes());
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                if !text.starts_with("# Netscape HTTP Cookie File") {
                    fail("'cookies' is not a valid Netscape formatted cookies file");
                }
                if !text.contains("\treddit_session\t") || text.contains("signedout") {
                    fail("[Reddit] 1abc2de: Account authentication is required. Use --cookies-from-browser or --cookies for the authentication.");
                }
                let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
                let _ = writeln!(f, ".reddit.com\tTRUE\t/\tTRUE\t0\tloid\twritten-back");
            }
        },
        // F5.4: refused with HTTP 403 without a session; with one it opens
        // unless the session is "nomember" (signed in, not allowed).
        "denied-auth" => {
            let cookies = arg_after(&args, "--cookies-from-browser").or_else(|| arg_after(&args, "--cookies"));
            match cookies {
                None => fail("[generic] Unable to download webpage: HTTP Error 403: Forbidden"),
                Some(src) if src.ends_with("nomember") => fail("[generic] Unable to download webpage: HTTP Error 403: Forbidden"),
                Some(_) => {}
            }
        }
        _ => {}
    }
    if args.iter().any(|a| a == "--cookies")
        && !scenario.starts_with("login-file")
        && scenario != "denied-auth"
    {
        fail("usage: a cookie file is only for the login-file scenarios");
    }
    if !metadata && args.iter().any(|a| a == "--sub-langs") {
        captions(&args, &scenario, &dir);
        return;
    }
    if metadata {
        if scenario == "playlist" {
            println!("{{\"_type\":\"playlist\",\"id\":\"pl\",\"title\":\"A list\",\"entries\":[{{}},{{}}]}}");
            return;
        }
        if args.iter().any(|a| a == "--write-thumbnail") {
            // A tiny JPEG-looking file (the engine checks the magic bytes).
            let _ = std::fs::write(
                dir.join("thumb.jpg"),
                [0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F'],
            );
        }
        let mut doc = serde_json::json!({
            "id": format!("fake-{scenario}"),
            "title": format!("Fake video {scenario}"),
            "extractor_key": "Generic",
            "webpage_url": url,
            "duration": 3.0,
            "uploader": "Fake Channel",
            "upload_date": "20261101",
            "formats": [
                {"format_id": "18", "ext": "mp4", "protocol": "https", "vcodec": "avc1.42001E", "acodec": "mp4a.40.2", "height": 360, "tbr": 500.0, "filesize": MEDIA.len(), "url": format!("https://cdn.example.com/v.mp4?sig={CANARY}"), "http_headers": {"Cookie": CANARY}},
                {"format_id": "137", "ext": "mp4", "protocol": "https", "vcodec": "avc1.640028", "acodec": "none", "height": 1080, "tbr": 4000.0, "filesize": 40000, "url": format!("https://cdn.example.com/137?sig={CANARY}")},
                {"format_id": "140", "ext": "m4a", "protocol": "https", "vcodec": "none", "acodec": "mp4a.40.2", "tbr": 128.0, "filesize": 2000, "url": format!("https://cdn.example.com/140?sig={CANARY}")}
            ],
            "http_headers": {"Cookie": format!("s={CANARY}")},
            "description": format!("secret {CANARY}")
        });
        let track = |ext: &str| serde_json::json!([{"ext": ext, "url": format!("https://cdn.example.com/sub?sig={CANARY}")}]);
        let (lang, subs, auto) = match scenario.as_str() {
            "captions-manual" => (
                "tr",
                serde_json::json!({"tr": track("vtt"), "live_chat": track("json")}),
                serde_json::json!({"tr-orig": track("vtt"), "en": track("vtt")}),
            ),
            "captions-youtube-tr" => (
                "tr",
                serde_json::json!({"tr": track("vtt"), "en": track("vtt")}),
                serde_json::json!({"tr-orig": track("vtt"), "de": track("vtt")}),
            ),
            "captions-youtube-en" => (
                "en",
                serde_json::json!({"live_chat": track("json")}),
                serde_json::json!({"en-orig": track("vtt"), "tr": track("vtt"), "en": track("vtt")}),
            ),
            "captions-auto" => (
                "en",
                serde_json::json!({}),
                serde_json::json!({"en-orig": track("vtt"), "de": track("vtt")}),
            ),
            "captions-bad" | "captions-missing" => (
                "en",
                serde_json::json!({"en": track("srv3")}),
                serde_json::json!({}),
            ),
            "captions-none" => (
                "en",
                serde_json::json!({}),
                serde_json::json!({"de": track("vtt")}),
            ),
            _ => ("", serde_json::json!({}), serde_json::json!({})),
        };
        if scenario.starts_with("captions-youtube") {
            // The caption files run longer than the 3 s clip.
            doc["duration"] = 30.0.into();
        }
        if !lang.is_empty() {
            doc["language"] = lang.into();
            doc["subtitles"] = subs;
            doc["automatic_captions"] = auto;
        }
        println!("{doc}");
        return;
    }

    // Download mode.
    let out = dir.join("media.mp4");
    let total = MEDIA.len() as u64;
    match scenario.as_str() {
        "flaky" => {
            let marker = dir.join("fake-attempt");
            if !marker.exists() {
                let _ = std::fs::write(&marker, b"1");
                progress(1000, Some(total), "downloading");
                fail("Unable to download video data: <urlopen error [Errno 101] Network is unreachable>");
            }
        }
        "slow" | "login-file-slow" => {
            let part = dir.join("media.mp4.part");
            let mut done = 0u64;
            loop {
                done += 1024;
                let _ = std::fs::write(&part, vec![0u8; done.min(1 << 20) as usize]);
                progress(done, Some(total * 100), "downloading");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        "corrupt" => {
            let _ = std::fs::write(&out, b"not a video at all");
            progress(18, Some(18), "finished");
            println!(
                "vv-file {}",
                serde_json::to_string(&out.to_string_lossy()).unwrap()
            );
            return;
        }
        "escape" => {
            // Reports a file outside the staging folder.
            let elsewhere = std::env::temp_dir().join("vv-fake-escape.mp4");
            let _ = std::fs::write(&elsewhere, MEDIA);
            println!(
                "vv-file {}",
                serde_json::to_string(&elsewhere.to_string_lossy()).unwrap()
            );
            return;
        }
        _ => {}
    }
    let unknown = scenario == "unknown-total";
    let parts = if scenario == "merge" { 2 } else { 1 };
    for _ in 0..parts {
        for step in 1..=4u64 {
            progress(total * step / 4, (!unknown).then_some(total), "downloading");
            std::thread::sleep(Duration::from_millis(20));
        }
        progress(total, (!unknown).then_some(total), "finished");
    }
    if parts == 2 {
        eprintln!("vv-postprocess {{\"status\":\"started\",\"postprocessor\":\"Merger\"}}");
        eprintln!("vv-postprocess {{\"status\":\"finished\",\"postprocessor\":\"Merger\"}}");
    }
    std::fs::write(&out, MEDIA).expect("write media");
    println!(
        "vv-file {}",
        serde_json::to_string(&out.to_string_lossy()).unwrap()
    );
}

/// Caption mode (F6.2): writes `caps.<track>.vtt` like
/// `-o subtitle:caps.%(ext)s` does, after checking the engine asked for the
/// right kind of track.
fn captions(args: &[String], scenario: &str, dir: &std::path::Path) {
    let track = arg_after(args, "--sub-langs").unwrap_or_default();
    let auto = args.iter().any(|a| a == "--write-auto-subs");
    let manual = args.iter().any(|a| a == "--write-subs");
    if !args.iter().any(|a| a == "--skip-download") || auto == manual {
        fail("usage: captions need --skip-download and one kind of track");
    }
    let body = match (scenario, track.as_str(), auto) {
        ("captions-manual", "tr", false) => "WEBVTT\n\n00:00:00.000 --> 00:00:01.200\nMerhaba <b>dünya</b>\n\n00:00:01.200 --> 00:00:02.900\nİkinci satır &amp; son\n".to_owned(),
        ("captions-auto", "en-orig", true) => "WEBVTT\nKind: captions\nLanguage: en\n\n00:00:00.100 --> 00:00:01.000 align:start position:0%\n \nhello<00:00:00.400><c> world</c>\n\n00:00:01.000 --> 00:00:01.010 align:start position:0%\nhello world\n \n\n00:00:01.010 --> 00:00:02.500 align:start position:0%\nhello world\nthis<00:00:01.300><c> is</c><c> fine</c>\n".to_owned(),
        ("captions-bad", "en", false) => "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nfine\n\n00:00:10.000 --> 00:00:12.000\nafter the end\n".to_owned(),
        ("captions-missing", "en", false) => return,
        ("captions-youtube-tr", "tr", false) => YOUTUBE_TR.to_owned(),
        ("captions-youtube-en", "en-orig", true) => YOUTUBE_EN_AUTO.to_owned(),
        _ => fail(&format!("usage: unexpected caption track {track} for {scenario}")),
    };
    std::fs::write(dir.join(format!("caps.{track}.vtt")), body).expect("write captions");
}
