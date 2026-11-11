//! `transcription` jobs: a transcript made on this PC with whisper.cpp
//! (STATE.md F6.2; SPEC AI-A02, §8.4 step 3, §11.5, §17; ADR-0028).
//!
//! 1. The item's own media file (original first, then a preview) is read
//!    by the pinned FFmpeg with the fixed input allowlists and written as
//!    16 kHz mono WAV into the job's staging folder.
//! 2. The checked `whisper-cli` runs on it with the model the user agreed
//!    to download (`ready`, same file and size as on record), on the CPU,
//!    writing JSON next to the WAV. Its progress lines become job progress;
//!    nothing it prints is stored or logged, because it prints the text.
//! 3. The JSON is read into segments ([`parse_output`]): times are
//!    whisper's own, empty and zero-length segments and whole-segment
//!    markers such as `[BLANK_AUDIO]` are dropped, the rest is checked
//!    against §11.5 with the WAV's length as the duration.
//! 4. Saved as a `generated_local` transcript (provider `whisper_cpp`,
//!    model id and pinned revision) in the transaction that ends the job.
//!    No speech at all: the job succeeds without a transcript.
//!
//! Nothing here uses the network. The staging folder is removed afterwards
//! whatever happened. Long files run in one piece; parts and checkpoints
//! come with F6.3.

use crate::context::{Context, Store, WhisperTool};
use crate::proc::{run_streaming, spawn_error_code, StdoutMode};
use crate::worker::{Stop, Work};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use vv_domain::error::ErrorCode;
use vv_persistence::jobs::{self, JobKind, Lease, NewJob, Progress};
use vv_persistence::library::{repo, AssetRole, IntegrityState, MediaKind, StorageRootStatus};
use vv_persistence::models::{self as model_rows, Installation, ModelStatus};
use vv_persistence::rusqlite::Connection;
use vv_persistence::transcripts::{self, NewSegment, Origin, Provenance, Status};
use vv_process::ManagedCommand;
use vv_ytdlp::outcome::{Exit, Stream};

/// Bumped when what a transcription job writes changes.
pub const TRANSCRIBE_CONFIG_VERSION: u32 = 1;

/// Languages the user can ask for; `auto` lets the model detect it.
/// Turkish and English are the app's focus (AI-A07); other languages work
/// with `auto`.
pub const LANGUAGES: &[&str] = &["auto", "tr", "en"];

/// The JSON whisper-cli writes for a long lecture is a few MB.
pub const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

/// `0xC0000135` STATUS_DLL_NOT_FOUND: Windows could not load a library the
/// executable needs (the Visual C++ runtime, ADR-0028).
const WIN_DLL_NOT_FOUND: i32 = 0xC000_0135_u32 as i32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub model_id: String,
    /// One of [`LANGUAGES`].
    pub language: String,
}

/// Queues a transcription (one open job per item, model and language).
pub fn enqueue(
    c: &mut Connection,
    item_id: &str,
    model_id: &str,
    language: &str,
    now: i64,
) -> jobs::Result<jobs::Enqueued> {
    let mut job = NewJob::new(
        JobKind::Transcription,
        format!("transcription:{item_id}:{model_id}:{language}:v{TRANSCRIBE_CONFIG_VERSION}"),
    );
    job.item_id = Some(item_id.to_owned());
    job.payload = serde_json::to_value(Payload {
        model_id: model_id.to_owned(),
        language: language.to_owned(),
    })
    .unwrap_or_default();
    // A failed run would fail the same way again; the user asks again.
    job.max_attempts = 1;
    jobs::enqueue(c, &job, now)
}

/// The media file a transcription reads: the item's original, else a
/// preview, video or audio, not known to be missing.
pub fn source_asset(c: &Connection, item_id: &str) -> Option<repo::MediaAsset> {
    let mut assets: Vec<_> = repo::assets_for_item(c, item_id)
        .ok()?
        .into_iter()
        .filter(|a| {
            matches!(a.role, AssetRole::Original | AssetRole::Preview)
                && matches!(a.media_kind, MediaKind::Video | MediaKind::Audio)
                && a.integrity_state != IntegrityState::Missing
        })
        .collect();
    assets.sort_by_key(|a| (a.role != AssetRole::Original, a.created_at));
    assets.into_iter().next()
}

/// The asset's file on disk, inside its storage root.
pub fn asset_path(c: &Connection, asset_id: &str) -> Result<PathBuf, ErrorCode> {
    let (root, storage_ref, status) = vv_persistence::library::view::media_location(c, asset_id)
        .map_err(|_| ErrorCode::InternalError)?
        .ok_or(ErrorCode::FileMissing)?;
    if status != StorageRootStatus::Available {
        return Err(ErrorCode::StorageRootUnavailable);
    }
    let root = std::fs::canonicalize(root).map_err(|_| ErrorCode::StorageRootUnavailable)?;
    let file = std::fs::canonicalize(vv_import::fsutil::resolve_ref(&root, &storage_ref))
        .map_err(|_| ErrorCode::FileMissing)?;
    (file.starts_with(&root) && file.is_file())
        .then_some(file)
        .ok_or(ErrorCode::FileMissing)
}

/// A model that may be used now: agreed to, hash-checked when installed,
/// still the catalog's entry, and its file still the recorded size.
pub fn usable_model(c: &Connection, models_dir: &Path, id: &str) -> Option<Installation> {
    let inst = model_rows::get(c, id).ok().flatten()?;
    let cat = crate::models::catalog();
    let spec = cat.model(id).and_then(|m| cat.spec(m))?;
    let len = std::fs::metadata(crate::models::model_path(models_dir, &inst))
        .ok()?
        .len();
    (inst.status == ModelStatus::Ready && inst.matches(&spec) && len as i64 == spec.size_bytes)
        .then_some(inst)
}

/// Threads for whisper.cpp: all cores but one, at most 8 (it gains little
/// beyond that on a CPU and the PC stays usable).
pub fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1))
        .unwrap_or(1)
        .clamp(1, 8)
}

/// whisper-cli argv (without the program). `out_base` gets `.json`.
pub fn whisper_args(
    model: &Path,
    wav: &Path,
    out_base: &Path,
    language: &str,
    threads: usize,
) -> Vec<OsString> {
    let threads = threads.to_string();
    let mut v: Vec<OsString> = vec![
        "-m".into(),
        model.as_os_str().to_owned(),
        "-f".into(),
        wav.as_os_str().to_owned(),
    ];
    v.extend(
        [
            "-l", language, "-t", &threads,
            // CPU only (SPEC §17.2); the pinned build has no GPU backend anyway.
            "-ng", // JSON file only; no logs, progress on stderr.
            "-oj", "-np", "-pp",
        ]
        .map(OsString::from),
    );
    v.push("-of".into());
    v.push(out_base.as_os_str().to_owned());
    v
}

/// `whisper_print_progress_callback: progress =  45%` -> 45.
pub fn parse_progress(line: &str) -> Option<i64> {
    let rest = line.split_once("progress =")?.1.trim();
    let n: i64 = rest.strip_suffix('%')?.trim().parse().ok()?;
    (0..=100).contains(&n).then_some(n)
}

/// What whisper-cli's `-oj` file says.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// The language it transcribed (`tr`, `en`; detected with `auto`).
    pub language: String,
    pub segments: Vec<NewSegment>,
}

#[derive(Deserialize)]
struct RawOutput {
    result: Option<RawResult>,
    #[serde(default)]
    transcription: Vec<RawSegment>,
}

#[derive(Deserialize)]
struct RawResult {
    language: Option<String>,
}

#[derive(Deserialize)]
struct RawSegment {
    offsets: RawOffsets,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct RawOffsets {
    from: i64,
    to: i64,
}

/// whisper-cli escapes only `"` and `\` in its JSON strings; a control
/// character in the text would make the file invalid JSON. Those inside
/// strings become spaces.
fn sanitize_json(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(text.len());
    let (mut in_str, mut esc) = (false, false);
    for ch in text.chars() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            } else if (ch as u32) < 0x20 {
                out.push(' ');
                continue;
            }
        } else if ch == '"' {
            in_str = true;
        }
        out.push(ch);
    }
    out
}

/// A whole segment that is only a marker whisper writes for non-speech:
/// `[BLANK_AUDIO]`, `[Music]`, `(applause)`.
fn is_marker(text: &str) -> bool {
    let t = text.trim();
    let wrapped = |o: char, c: char| {
        t.strip_prefix(o)
            .and_then(|r| r.strip_suffix(c))
            .is_some_and(|inner| inner.chars().all(|x| x != o && x != c))
    };
    wrapped('[', ']') || wrapped('(', ')')
}

/// A BCP 47-looking tag (`tr`, `en`, `zh`), else `und`.
fn language_tag(s: Option<&str>) -> String {
    match s.map(str::trim) {
        Some(l)
            if (2..=8).contains(&l.len())
                && l.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
                && l != "auto" =>
        {
            l.to_owned()
        }
        _ => "und".to_owned(),
    }
}

/// Reads whisper-cli's JSON. `asked` is the language it was told to use.
pub fn parse_output(raw: &[u8], asked: &str) -> Result<Output, String> {
    let doc: RawOutput = serde_json::from_str(&sanitize_json(raw)).map_err(|e| {
        format!(
            "transcriber output is not readable JSON (line {})",
            e.line()
        )
    })?;
    let detected = doc.result.and_then(|r| r.language);
    let language = if asked == "auto" {
        language_tag(detected.as_deref())
    } else {
        language_tag(Some(asked))
    };
    let segments = doc
        .transcription
        .into_iter()
        .map(|mut s| {
            // A `\u0000`-style escape decodes to a control character; like
            // caption text, stored text has none (found by tests/fuzz.rs).
            if s.text.chars().any(char::is_control) {
                s.text = s
                    .text
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
            }
            s
        })
        .filter(|s| {
            s.offsets.to > s.offsets.from && !s.text.trim().is_empty() && !is_marker(&s.text)
        })
        .map(|s| NewSegment::new(s.offsets.from, s.offsets.to, s.text.trim()))
        .collect();
    Ok(Output { language, segments })
}

/// Length of a 16 kHz mono 16-bit PCM WAV from its `data` chunk.
pub fn wav_duration_ms(bytes: &[u8]) -> Option<i64> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        if id == b"data" {
            // FFmpeg leaves the size at 0xFFFFFFFF only when writing to a
            // pipe; we write a file, but take what is really there.
            let avail = bytes.len().saturating_sub(i + 8);
            let n = len.min(avail);
            return Some((n as i64) * 1000 / 32_000);
        }
        i += 8 + len + (len & 1);
    }
    None
}

/// `done` is a percentage; a new phase is recorded at once (`force`).
fn phase<S: Store>(w: &Work<'_, S>, name: &str, done: Option<i64>, force: bool) {
    w.progress(
        &Progress {
            phase: Some(name.to_owned()),
            units_done: done,
            units_total: done.map(|_| 100),
            ..Progress::default()
        },
        force,
    );
}

pub fn run<S: Store>(cx: &Context<S>, lease: Lease) {
    let w = Work::new(cx, lease);
    let Some(item_id) = w.item_id().map(str::to_owned) else {
        w.end_error(
            ErrorCode::InternalError,
            Some("transcription job without item"),
        );
        return;
    };
    let Ok(p) = serde_json::from_value::<Payload>(w.lease.job.payload.clone()) else {
        w.end_error(
            ErrorCode::InternalError,
            Some("transcription payload unreadable"),
        );
        return;
    };
    if !LANGUAGES.contains(&p.language.as_str()) {
        w.end_error(ErrorCode::InputRejected, Some("language not offered"));
        return;
    }
    let found = cx.store.read(|c| {
        let item = repo::get_item(c, &item_id).ok().flatten()?;
        let asset = source_asset(c, &item_id);
        let path = asset.as_ref().map(|a| asset_path(c, &a.id));
        let model = usable_model(c, &cx.dirs.models, &p.model_id);
        Some((item, asset, path, model))
    });
    let Some((item, asset, path, model)) = found else {
        return; // The item is gone; its jobs went with it.
    };
    let (Some(asset), Some(path)) = (asset, path) else {
        w.end_error(ErrorCode::FileMissing, Some("item has no media file"));
        return;
    };
    let path = match path {
        Ok(p) => p,
        Err(code) => {
            w.end_error(code, None);
            return;
        }
    };
    let Some(model) = model else {
        w.end_error(ErrorCode::ModelMissing, None);
        return;
    };
    let tools = cx.tools();
    let (Some(media), Some(whisper)) = (tools.media.clone(), tools.whisper.clone()) else {
        w.end_error(ErrorCode::ToolMissing, None);
        return;
    };
    let dir = cx
        .dirs
        .staging
        .join(format!("asr-{}-{}", w.id(), w.generation()));
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir_all(&dir).is_err() {
        w.end_error(ErrorCode::PathNotWritable, None);
        return;
    }
    let job = Job {
        item: &item,
        asset: &asset,
        input: &path,
        model: &model,
        ffmpeg: &media.ffmpeg,
        whisper: &whisper,
        language: &p.language,
        dir: &dir,
    };
    run_in(&w, &job);
    let _ = std::fs::remove_dir_all(&dir);
    cx.events.item_changed(&item_id);
}

struct Job<'a> {
    item: &'a repo::LibraryItem,
    asset: &'a repo::MediaAsset,
    input: &'a Path,
    model: &'a Installation,
    ffmpeg: &'a Path,
    whisper: &'a WhisperTool,
    language: &'a str,
    dir: &'a Path,
}

/// Runs one tool to the end; `Err` when the job was ended here.
fn run_tool<S: Store>(
    w: &Work<'_, S>,
    exe: &Path,
    args: Vec<OsString>,
    dir: &Path,
    mut on_err_line: impl FnMut(&str),
) -> Result<Exit, ()> {
    let mut cmd = ManagedCommand::new(exe);
    cmd.args(&args).current_dir(dir);
    let r = run_streaming(
        cmd,
        StdoutMode::Lines,
        None,
        |s, l| {
            if s == Stream::Stderr {
                on_err_line(l);
            }
        },
        || w.should_stop(),
    );
    match r {
        Err(e) => {
            w.end_error(spawn_error_code(&e), None);
            Err(())
        }
        Ok(s) if s.stopped => {
            w.end_stopped(w.stopped().unwrap_or(Stop::Cancel));
            Err(())
        }
        Ok(s) => Ok(s.exit),
    }
}

fn run_in<S: Store>(w: &Work<'_, S>, j: &Job<'_>) {
    // 1. Audio.
    phase(w, "transcribe_audio", None, true);
    let wav = j.dir.join("audio.wav");
    let Ok(args) = vv_ffmpeg::args::audio_wav_args(j.input, &wav) else {
        w.end_error(ErrorCode::InternalError, Some("media path is not absolute"));
        return;
    };
    let mut no_audio = false;
    let Ok(exit) = run_tool(w, j.ffmpeg, args, j.dir, |l| {
        // "Stream map '0:a:0' matches no streams."
        no_audio |= l.contains("matches no streams");
    }) else {
        return;
    };
    let wav_ms = std::fs::read(&wav).ok().and_then(|b| wav_duration_ms(&b));
    let wav_ms = match (exit, wav_ms) {
        (Exit::Code(0), Some(ms)) if ms > 0 => ms,
        _ => {
            let msg = if no_audio {
                "the media has no audio"
            } else {
                "audio could not be read"
            };
            w.end_error(ErrorCode::MediaCorrupt, Some(msg));
            return;
        }
    };
    // 2. Transcribe.
    phase(w, "transcribe", Some(0), true);
    let out_base = j.dir.join("out");
    let model_file = crate::models::model_path(&w.cx.dirs.models, j.model);
    let args = whisper_args(&model_file, &wav, &out_base, j.language, threads());
    let Ok(exit) = run_tool(w, &j.whisper.exe, args, j.dir, |l| {
        if let Some(n) = parse_progress(l) {
            phase(w, "transcribe", Some(n), n == 100);
        }
    }) else {
        return;
    };
    match exit {
        Exit::Code(0) => {}
        Exit::Code(WIN_DLL_NOT_FOUND) => {
            w.end_error(
                ErrorCode::ToolMissing,
                Some("whisper-cli could not load a library (Visual C++ runtime?)"),
            );
            return;
        }
        Exit::Code(n) => {
            w.end_error(
                ErrorCode::InternalError,
                Some(&format!("whisper-cli exited {n}")),
            );
            return;
        }
        _ => {
            w.end_error(ErrorCode::InternalError, Some("whisper-cli was killed"));
            return;
        }
    }
    let json = out_base.with_extension("json");
    let raw = match std::fs::metadata(&json) {
        Ok(m) if m.len() <= MAX_OUTPUT_BYTES => std::fs::read(&json).ok(),
        _ => None,
    };
    let Some(raw) = raw else {
        w.end_error(
            ErrorCode::InternalError,
            Some("transcriber wrote no result"),
        );
        return;
    };
    // 3. Check.
    phase(w, "transcribe_save", None, true);
    let out = match parse_output(&raw, j.language) {
        Ok(o) => o,
        Err(e) => {
            w.end_error(ErrorCode::InternalError, Some(&e));
            return;
        }
    };
    if out.segments.is_empty() {
        finish(w, j, None);
        return;
    }
    let segs = match transcripts::validate(&out.segments, Some(wav_ms)) {
        Ok(s) => s,
        Err(e) => {
            w.end_error(ErrorCode::InternalError, Some(&e.to_string()));
            return;
        }
    };
    let origin = Origin {
        item_id: j.item.id.clone(),
        asset_id: Some(j.asset.id.clone()),
        language: out.language,
        provenance: Provenance::GeneratedLocal,
        provider: "whisper_cpp".into(),
        model_id: Some(j.model.spec.id.clone()),
        model_version: Some(j.model.spec.version.clone()),
        source_fingerprint: fingerprint(j),
        caption_kind: None,
    };
    finish(w, j, Some((origin, segs, wav_ms)));
}

/// What the text was made from: the media file, the model file and the
/// transcriber, and the language asked for. A change to any is a new run.
pub fn fingerprint_of(
    asset: &repo::MediaAsset,
    model_sha256: &str,
    whisper_version: &str,
    language: &str,
) -> String {
    let media = match (&asset.content_hash, asset.size_bytes) {
        (Some(h), _) => format!("sha256:{}", h.chars().take(64).collect::<String>()),
        (None, Some(n)) => format!("size:{n}"),
        (None, None) => "unhashed".into(),
    };
    format!(
        "asset:{}:{media}:model:{}:whisper.cpp:{}:lang:{language}",
        asset.id,
        model_sha256.chars().take(16).collect::<String>(),
        // From the tool's own `--version` output: never sliced by bytes.
        whisper_version.chars().take(16).collect::<String>(),
    )
}

fn fingerprint(j: &Job<'_>) -> String {
    fingerprint_of(
        j.asset,
        &j.model.spec.sha256,
        &j.whisper.version,
        j.language,
    )
}

type Found = (Origin, Vec<NewSegment>, i64);

fn finish<S: Store>(w: &Work<'_, S>, j: &Job<'_>, found: Option<Found>) {
    let cx = w.cx;
    let now = cx.now();
    let (id, g) = (w.id().to_owned(), w.generation());
    let r = cx.store.write(|c| {
        jobs::succeed_with(c, &id, g, now, |tx| {
            if let Some((origin, segs, covered)) = &found {
                transcripts::save(
                    tx,
                    origin,
                    Status::Ready,
                    Some(*covered),
                    segs,
                    Some(*covered),
                    now,
                )
                .map_err(|e| match e {
                    transcripts::TranscriptError::Sqlite(e) => jobs::JobError::Sqlite(e),
                    _ => jobs::JobError::Invalid("transcript"),
                })?;
            }
            Ok(())
        })
    });
    match r {
        Ok(()) | Err(jobs::JobError::Stale) => {}
        Err(jobs::JobError::InvalidTransition { .. }) => {
            w.end_stopped(Stop::Cancel);
        }
        Err(_) => {
            w.end_error(ErrorCode::InternalError, Some("transcript not saved"));
        }
    }
    cx.events.job_changed(&id, Some(&j.item.id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines() {
        assert_eq!(
            parse_progress("whisper_print_progress_callback: progress =  45%"),
            Some(45)
        );
        assert_eq!(
            parse_progress("whisper_print_progress_callback: progress = 100%"),
            Some(100)
        );
        assert_eq!(parse_progress("progress = 101%"), None);
        assert_eq!(parse_progress("main: processing 'audio.wav'"), None);
    }

    #[test]
    fn output_is_read_with_whispers_own_times() {
        let raw = br#"{
	"systeminfo": "AVX = 1",
	"model": {"type": "tiny"},
	"params": {"model": "m", "language": "auto", "translate": false},
	"result": {"language": "tr"},
	"transcription": [
		{"timestamps": {"from": "00:00:00,000", "to": "00:00:02,500"}, "offsets": {"from": 0, "to": 2500}, "text": " Merhaba \"dunya\"."},
		{"timestamps": {}, "offsets": {"from": 2500, "to": 2500}, "text": " zero length"},
		{"timestamps": {}, "offsets": {"from": 2500, "to": 4000}, "text": " [BLANK_AUDIO]"},
		{"timestamps": {}, "offsets": {"from": 4000, "to": 6000}, "text": " tab	inside"},
		{"timestamps": {}, "offsets": {"from": 6000, "to": 7000}, "text": "   "}
	]
}"#;
        let o = parse_output(raw, "auto").unwrap();
        assert_eq!(o.language, "tr");
        assert_eq!(
            o.segments,
            vec![
                NewSegment::new(0, 2500, "Merhaba \"dunya\"."),
                NewSegment::new(4000, 6000, "tab inside"),
            ]
        );
        // Asked for English: that is the language on record.
        assert_eq!(parse_output(raw, "en").unwrap().language, "en");
        assert!(parse_output(b"not json", "auto").is_err());
        let odd = br#"{"result": {"language": "<script>"}, "transcription": []}"#;
        assert_eq!(parse_output(odd, "auto").unwrap().language, "und");
    }

    #[test]
    fn escaped_control_characters_do_not_reach_the_text() {
        let raw = br#"{"result":{"language":"tr"},"transcription":[{"offsets":{"from":0,"to":900},"text":" a\u0000b\u0007 c "}]}"#;
        let o = parse_output(raw, "auto").unwrap();
        assert_eq!(o.segments[0].text, "a b  c");
    }

    #[test]
    fn markers() {
        for m in ["[BLANK_AUDIO]", " [Music] ", "(applause)", "[ Silence ]"] {
            assert!(is_marker(m), "{m}");
        }
        for t in ["[a] and [b]", "Hello [laughs]", "(a) b (c)", "[", "x"] {
            assert!(!is_marker(t), "{t}");
        }
    }

    #[test]
    fn wav_length_from_the_data_chunk() {
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF\0\0\0\0WAVE");
        w.extend_from_slice(b"fmt \x10\0\0\0");
        w.extend_from_slice(&[0u8; 16]);
        w.extend_from_slice(b"LIST\x03\0\0\0abc\0");
        w.extend_from_slice(b"data");
        w.extend_from_slice(&64_000u32.to_le_bytes());
        w.extend_from_slice(&vec![0u8; 64_000]);
        assert_eq!(wav_duration_ms(&w), Some(2000));
        w.truncate(w.len() - 32_000);
        assert_eq!(wav_duration_ms(&w), Some(1000));
        assert_eq!(wav_duration_ms(b"RIFF....AVI "), None);
    }

    #[test]
    fn whisper_argv_keeps_paths_whole() {
        let base = if cfg!(windows) { r"C:\a b\x" } else { "/a b/x" };
        let p = |s: &str| PathBuf::from(format!("{base}{s}"));
        let a = whisper_args(&p("m.bin"), &p("audio.wav"), &p("out"), "tr", 4);
        let s: Vec<String> = a.into_iter().map(|x| x.into_string().unwrap()).collect();
        assert_eq!(
            s[0..4],
            [
                "-m".to_string(),
                format!("{base}m.bin"),
                "-f".into(),
                format!("{base}audio.wav")
            ]
        );
        assert!(s.windows(2).any(|w| w == ["-l", "tr"]));
        assert!(s.windows(2).any(|w| w == ["-t", "4"]));
        assert!(s.contains(&"-ng".to_string()) && s.contains(&"-oj".to_string()));
        assert_eq!(s[s.len() - 2..], ["-of".to_string(), format!("{base}out")]);
        assert!((1..=8).contains(&threads()));
    }
}
