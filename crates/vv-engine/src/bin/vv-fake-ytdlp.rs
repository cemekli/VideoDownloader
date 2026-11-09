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
        _ => {}
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
        let doc = serde_json::json!({
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
        "slow" => {
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
