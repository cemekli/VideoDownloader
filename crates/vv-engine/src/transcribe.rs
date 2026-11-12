//! `transcription` jobs: a transcript made on this PC with whisper.cpp
//! (STATE.md F6.2; SPEC AI-A02, §8.4 step 3, §11.5, §17; ADR-0028).
//!
//! 1. The item's own media file (original first, then a preview) is read
//!    by the pinned FFmpeg with the fixed input allowlists and written as
//!    16 kHz mono WAV into the job's staging folder.
//! 2. A long recording is split into parts of about ten minutes, cut at
//!    the quietest moment near each even split ([`plan_chunks`]; F6.3,
//!    AI-A03). Parts touch and never overlap: no text twice, no shifted
//!    times.
//! 3. For each part, the checked `whisper-cli` runs on it with the model
//!    the user agreed to download (`ready`, same file and size as on
//!    record), on the CPU, writing JSON. Its progress lines become job
//!    progress; nothing it prints is stored or logged, because it prints
//!    the text. With `auto`, the first part's language is used for the rest.
//! 4. The JSON is read into segments ([`parse_output`]): times are
//!    whisper's own, empty and zero-length segments and whole-segment
//!    markers such as `[BLANK_AUDIO]` are dropped, the rest is checked
//!    against §11.5 with the part's length, then moved to the recording's
//!    time.
//! 5. After each part but the last, a [`Checkpoint`] (fingerprint of
//!    media, model, transcriber and language; parts; segments so far) is
//!    written in the job's staging folder and the text so far is saved as a
//!    `partial` transcript with the part's end as `covered_ms` (AI-A04,
//!    AC-23). A run of the same job after a restart continues after the
//!    last finished part; a checkpoint that does not fit is not used and
//!    the job starts over. Cancel keeps the partial transcript.
//! 6. The whole is saved as a `generated_local` transcript (provider
//!    `whisper_cpp`, model id and pinned revision) in the transaction that
//!    ends the job, replacing the partial one. No speech at all: the job
//!    succeeds without a transcript.
//!
//! Before any of that (F6.6, AI-A05): a model that does not fit this PC's
//! memory ([`crate::resources::fit`]) runs only when the user said "use it
//! anyway" for this job; staging must have room for the WAV. FFmpeg and
//! whisper run below normal priority, whisper with the threads the user's
//! CPU limit allows.
//!
//! Nothing here uses the network. The staging folder is removed when the
//! job ends; it stays only when the lease was lost (the job runs again).

use crate::context::{Context, Store, WhisperTool};
use crate::proc::{run_streaming, spawn_error_code, StdoutMode};
use crate::resources::{self, CpuUse, Fit};
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

/// Bumped when what a transcription job writes changes (2: segment starts
/// from DTW word times).
pub const TRANSCRIBE_CONFIG_VERSION: u32 = 2;

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
    /// The user saw that the model does not fit this PC's memory and asked
    /// to run it anyway (AI-A05). Older jobs lack it: `false`.
    #[serde(default)]
    pub allow_low_memory: bool,
}

/// Queues a transcription (one open job per item, model and language).
pub fn enqueue(
    c: &mut Connection,
    item_id: &str,
    model_id: &str,
    language: &str,
    allow_low_memory: bool,
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
        allow_low_memory,
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

/// Threads for whisper.cpp on this PC under the user's CPU limit (F6.6).
pub fn threads(u: CpuUse) -> usize {
    resources::threads(u, resources::cores())
}

/// Memory a catalog model needs on this PC, and whether it fits now.
pub fn model_fit(model_id: &str, mem: Option<resources::Memory>) -> Option<(u64, Fit)> {
    let m = crate::models::catalog().model(model_id)?;
    let need = resources::need_mb(m.upstream_memory_mb);
    Some((need, resources::fit(need, mem)))
}

/// whisper.cpp's alignment-heads preset for a catalog model, for
/// DTW token timestamps (`-dtw`); `None` for a model it has none for.
pub fn dtw_preset(model_id: &str) -> Option<&'static str> {
    match model_id {
        "whisper-tiny" => Some("tiny"),
        "whisper-base" => Some("base"),
        "whisper-small" => Some("small"),
        "whisper-medium" => Some("medium"),
        _ => None,
    }
}

/// whisper-cli argv (without the program). `out_base` gets `.json`.
///
/// With a `dtw` preset whisper also writes each token's time from the
/// attention alignment (`-ojf -dtw`); [`parse_output`] starts a segment at
/// its first spoken word instead of where whisper's window began (a 5 s
/// intro was put on the first line at 0:00). whisper.cpp turns DTW off when
/// flash attention is on, hence `-nfa`.
pub fn whisper_args(
    model: &Path,
    wav: &Path,
    out_base: &Path,
    language: &str,
    threads: usize,
    dtw: Option<&str>,
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
    if let Some(preset) = dtw {
        v.extend(["-ojf", "-dtw", preset, "-nfa"].map(OsString::from));
    }
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
    /// Only with `-ojf`.
    #[serde(default)]
    tokens: Vec<RawToken>,
}

#[derive(Deserialize)]
struct RawToken {
    #[serde(default)]
    text: String,
    /// DTW time in 10 ms units, -1 when not computed.
    #[serde(default)]
    t_dtw: Option<f64>,
}

/// Where the first spoken word of a segment starts, by DTW, in ms.
/// whisper's own segment start is off by a second or more either way, so
/// the word may move it up to [`MAX_EARLIER_MS`] earlier (never before
/// `floor`, the previous segment's start) or later (never past shortly
/// before its end). Special tokens (`[_BEG_]`, `[_TT_150]`) and punctuation
/// are skipped; without a DTW time whisper's start stays.
fn spoken_start(s: &RawSegment, floor: i64) -> i64 {
    let (from, to) = (s.offsets.from, s.offsets.to);
    let word = s
        .tokens
        .iter()
        .find(|t| !t.text.starts_with("[_") && t.text.chars().any(char::is_alphanumeric));
    match word.and_then(|t| t.t_dtw) {
        Some(c) if (0.0..1e9).contains(&c) => {
            let ms = (c * 10.0).round() as i64;
            let lo = (from - MAX_EARLIER_MS).max(floor).max(0).min(from);
            let hi = (to - MIN_SEGMENT_MS).max(from);
            ms.clamp(lo, hi)
        }
        _ => from,
    }
}

/// A segment keeps at least this much time after its start is moved.
const MIN_SEGMENT_MS: i64 = 200;
/// How far before whisper's start a segment's first word may move it.
const MAX_EARLIER_MS: i64 = 1500;

/// Segments with starts from DTW: each starts at its first word, and one
/// that now runs into the next ends where the next begins.
fn aligned(raw: Vec<RawSegment>) -> Vec<NewSegment> {
    let mut out: Vec<NewSegment> = Vec::with_capacity(raw.len());
    for s in &raw {
        let floor = out.last().map_or(0, |p| p.start_ms + MIN_SEGMENT_MS);
        let start = spoken_start(s, floor);
        if let Some(p) = out.last_mut() {
            if p.end_ms > start {
                p.end_ms = start.max(p.start_ms + 1);
            }
        }
        out.push(NewSegment::new(start, s.offsets.to, s.text.trim()));
    }
    out
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
        .collect();
    Ok(Output {
        language,
        segments: aligned(segments),
    })
}

/// Where the `data` chunk of a WAV starts and how many bytes it has.
fn wav_data(head: &[u8], file_len: u64) -> Option<(u64, u64)> {
    if head.len() < 12 || &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    while i + 8 <= head.len() {
        let id = &head[i..i + 4];
        let len = u32::from_le_bytes(head[i + 4..i + 8].try_into().ok()?) as u64;
        if id == b"data" {
            // FFmpeg leaves the size at 0xFFFFFFFF only when writing to a
            // pipe; we write a file, but take what is really there.
            let start = (i + 8) as u64;
            let avail = file_len.saturating_sub(start);
            return Some((start, len.min(avail)));
        }
        i += 8 + len as usize + (len & 1) as usize;
    }
    None
}

/// Length of a 16 kHz mono 16-bit PCM WAV from its `data` chunk.
pub fn wav_duration_ms(bytes: &[u8]) -> Option<i64> {
    let (_, n) = wav_data(bytes, bytes.len() as u64)?;
    Some(n as i64 / BYTES_PER_MS)
}

/// 16 kHz × 16-bit mono.
const BYTES_PER_MS: i64 = 32;

/// The job's WAV, read a piece at a time (a two-hour file is 230 MB).
pub struct Wav {
    file: std::fs::File,
    data_start: u64,
    /// Whole milliseconds of audio.
    pub ms: i64,
}

impl Wav {
    pub fn open(path: &Path) -> Option<Wav> {
        use std::io::Read;
        let mut file = std::fs::File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        let mut head = vec![0u8; 4096.min(len as usize)];
        file.read_exact(&mut head).ok()?;
        let (data_start, n) = wav_data(&head, len)?;
        Some(Wav {
            file,
            data_start,
            ms: n as i64 / BYTES_PER_MS,
        })
    }

    /// The samples from `from` to `to` ms, as little-endian bytes.
    pub fn read(&mut self, from: i64, to: i64) -> std::io::Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let (from, to) = (from.clamp(0, self.ms), to.clamp(0, self.ms));
        let mut buf = vec![0u8; ((to - from).max(0) * BYTES_PER_MS) as usize];
        self.file.seek(SeekFrom::Start(
            self.data_start + (from * BYTES_PER_MS) as u64,
        ))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// Writes `pcm` (16 kHz mono s16le) as a WAV file.
fn write_wav(path: &Path, pcm: &[u8]) -> std::io::Result<()> {
    let n = pcm.len() as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + n).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&16_000u32.to_le_bytes());
    out.extend_from_slice(&32_000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(pcm);
    std::fs::write(path, out)
}

/// Default length of one part of a long recording (F6.3, AI-A03). Each
/// part is a separate whisper-cli run and a checkpoint: a restart loses at
/// most one part (about 5 minutes with Medium on a laptop CPU).
pub const DEFAULT_CHUNK_MS: i64 = 10 * 60_000;
/// How far a cut may move from its even spot to find a pause.
const MAX_SHIFT_MS: i64 = 30_000;
/// The span whose loudness decides where the quietest moment is.
const QUIET_SPAN_MS: i64 = 400;
const QUIET_STEP_MS: i64 = 50;

/// Splits `total_ms` of audio into parts of about `chunk_ms`, cutting at the
/// quietest moment near each even split so a word is rarely cut in two.
/// Parts touch and never overlap, so no text comes twice and no time is
/// shifted (AI-A03). Up to 1.5 parts' worth stays one part. `audio(from,
/// to)` gives the samples of a span; unreadable spans keep the even cut.
pub fn plan_chunks(
    total_ms: i64,
    chunk_ms: i64,
    mut audio: impl FnMut(i64, i64) -> Option<Vec<u8>>,
) -> Vec<(i64, i64)> {
    if chunk_ms <= 0 || total_ms <= chunk_ms * 3 / 2 {
        return vec![(0, total_ms.max(0))];
    }
    let n = (total_ms + chunk_ms - 1) / chunk_ms;
    let shift = MAX_SHIFT_MS.min(chunk_ms / 4);
    let mut cuts = vec![0];
    for i in 1..n {
        let even = total_ms * i / n;
        let (lo, hi) = (even - shift, even + shift);
        let cut = audio(lo, hi)
            .map(|pcm| quietest(&pcm, lo, even))
            .unwrap_or(even);
        cuts.push(cut);
    }
    cuts.push(total_ms);
    cuts.windows(2).map(|w| (w[0], w[1])).collect()
}

/// The middle of the quietest span in `pcm` (which starts at `start` ms);
/// on a tie the one nearest `even`.
fn quietest(pcm: &[u8], start: i64, even: i64) -> i64 {
    let span = (QUIET_SPAN_MS * BYTES_PER_MS) as usize;
    let step = (QUIET_STEP_MS * BYTES_PER_MS) as usize;
    let mut best: Option<(i64, i64, i64)> = None; // (energy, distance, ms)
    let mut at = 0usize;
    while at + span <= pcm.len() {
        let energy: i64 = pcm[at..at + span]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| {
                let s = i16::from_le_bytes(*b) as i64;
                s * s
            })
            .sum();
        let ms = start + (at + span / 2) as i64 / BYTES_PER_MS;
        let key = (energy, (ms - even).abs(), ms);
        if best.is_none_or(|b| (key.0, key.1) < (b.0, b.1)) {
            best = Some(key);
        }
        at += step;
    }
    best.map_or(even, |b| b.2)
}

/// What a transcription has done so far, kept next to its WAV in the
/// job's staging folder after every part (AI-A04). It is used again only
/// for the same job and when it says the same input, model, transcriber,
/// language and parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub version: u32,
    /// [`fingerprint_of`]: media, model file, whisper.cpp, language asked.
    pub fingerprint: String,
    pub wav_ms: i64,
    pub chunks: Vec<(i64, i64)>,
    /// Parts finished, from the start.
    pub done: usize,
    /// The language found in the first part (with `auto`), used for the rest.
    pub language: Option<String>,
    /// Segments of the finished parts, in whole-recording time.
    pub segments: Vec<(i64, i64, String)>,
}

const CHECKPOINT_FILE: &str = "checkpoint.json";

fn read_checkpoint(dir: &Path) -> Option<Checkpoint> {
    let raw = std::fs::read(dir.join(CHECKPOINT_FILE)).ok()?;
    serde_json::from_slice::<Checkpoint>(&raw)
        .ok()
        .filter(|c| c.version == TRANSCRIBE_CONFIG_VERSION && c.done <= c.chunks.len())
}

/// Written beside and renamed over, so a crash leaves the old or the new.
fn write_checkpoint(dir: &Path, cp: &Checkpoint) -> std::io::Result<()> {
    let tmp = dir.join("checkpoint.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(cp).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, dir.join(CHECKPOINT_FILE))
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

/// The job's staging folder: one per job, so a run after a restart finds
/// the checkpoint of the run before it.
pub fn staging_dir(staging: &Path, job_id: &str) -> PathBuf {
    staging.join(format!("asr-{job_id}"))
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
    // AI-A05: a model this PC cannot hold is never loaded silently.
    if !p.allow_low_memory {
        if let Some((_, Fit::TooBig)) = model_fit(&p.model_id, resources::memory()) {
            w.end_error(
                ErrorCode::ResourceLimit,
                Some("the model needs more memory than this PC has"),
            );
            return;
        }
    }
    let cpu = cx
        .store
        .read(|c| vv_persistence::settings::get(c).map(|s| s.transcription_cpu))
        .unwrap_or_default();
    let tools = cx.tools();
    let (Some(media), Some(whisper)) = (tools.media.clone(), tools.whisper.clone()) else {
        w.end_error(ErrorCode::ToolMissing, None);
        return;
    };
    let dir = staging_dir(&cx.dirs.staging, w.id());
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
        threads: threads(cpu),
    };
    // A lost lease means the job runs again (after a restart or a reclaim):
    // its folder and checkpoint stay for that run. Otherwise it ended.
    if run_in(&w, &job) != End::Lost {
        let _ = std::fs::remove_dir_all(&dir);
    }
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
    threads: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// Finished, failed or cancelled.
    Ended,
    /// The lease went away mid-run; another run continues.
    Lost,
}

/// Ends the attempt for the stop the worker saw.
fn stopped<S: Store>(w: &Work<'_, S>) -> End {
    let s = w.stopped().unwrap_or(Stop::Cancel);
    w.end_stopped(s);
    if s == Stop::Lost {
        End::Lost
    } else {
        End::Ended
    }
}

fn fail<S: Store>(w: &Work<'_, S>, code: ErrorCode, msg: Option<&str>) -> End {
    w.end_error(code, msg);
    End::Ended
}

/// Runs one tool to the end; `Err` when the job was ended here.
fn run_tool<S: Store>(
    w: &Work<'_, S>,
    exe: &Path,
    args: Vec<OsString>,
    dir: &Path,
    mut on_err_line: impl FnMut(&str),
) -> Result<Exit, End> {
    let mut cmd = ManagedCommand::new(exe);
    cmd.args(&args).current_dir(dir).low_priority();
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
        Err(e) => Err(fail(w, spawn_error_code(&e), None)),
        Ok(s) if s.stopped => Err(stopped(w)),
        Ok(s) => Ok(s.exit),
    }
}

/// FFmpeg writes the job's WAV; its length, or the job is ended.
fn make_wav<S: Store>(w: &Work<'_, S>, j: &Job<'_>, wav: &Path) -> Result<i64, End> {
    let Ok(args) = vv_ffmpeg::args::audio_wav_args(j.input, wav) else {
        return Err(fail(
            w,
            ErrorCode::InternalError,
            Some("media path is not absolute"),
        ));
    };
    let mut no_audio = false;
    let exit = run_tool(w, j.ffmpeg, args, j.dir, |l| {
        // "Stream map '0:a:0' matches no streams."
        no_audio |= l.contains("matches no streams");
    })?;
    match (exit, Wav::open(wav).map(|v| v.ms)) {
        (Exit::Code(0), Some(ms)) if ms > 0 => Ok(ms),
        _ => {
            let msg = if no_audio {
                "the media has no audio"
            } else {
                "audio could not be read"
            };
            Err(fail(w, ErrorCode::MediaCorrupt, Some(msg)))
        }
    }
}

/// The checkpoint of an earlier run of this job, if it fits this run and
/// its WAV is still there.
fn resumable(j: &Job<'_>, wav: &Path) -> Option<Checkpoint> {
    let cp = read_checkpoint(j.dir)?;
    let ms = Wav::open(wav)?.ms;
    (cp.fingerprint == fingerprint(j) && cp.wav_ms == ms).then_some(cp)
}

fn run_in<S: Store>(w: &Work<'_, S>, j: &Job<'_>) -> End {
    let wav_path = j.dir.join("audio.wav");
    // 1. Audio and parts, or what an earlier run of this job left.
    let mut cp = match resumable(j, &wav_path) {
        Some(cp) => cp,
        None => {
            let _ = std::fs::remove_file(j.dir.join(CHECKPOINT_FILE));
            phase(w, "transcribe_audio", None, true);
            let duration = j.asset.duration_ms.or(j.item.duration_ms).unwrap_or(0);
            let need = resources::audio_disk_bytes(duration, j.whisper.chunk_ms);
            if crate::storage::free_bytes(j.dir).is_some_and(|free| free < need) {
                return fail(w, ErrorCode::DiskFull, Some("no room for the audio"));
            }
            let wav_ms = match make_wav(w, j, &wav_path) {
                Ok(ms) => ms,
                Err(end) => return end,
            };
            let Some(mut wav) = Wav::open(&wav_path) else {
                return fail(w, ErrorCode::MediaCorrupt, Some("audio could not be read"));
            };
            let chunks = plan_chunks(wav_ms, j.whisper.chunk_ms, |a, b| wav.read(a, b).ok());
            Checkpoint {
                version: TRANSCRIBE_CONFIG_VERSION,
                fingerprint: fingerprint(j),
                wav_ms,
                chunks,
                done: 0,
                language: None,
                segments: Vec::new(),
            }
        }
    };
    let Some(mut wav) = Wav::open(&wav_path) else {
        return fail(w, ErrorCode::MediaCorrupt, Some("audio could not be read"));
    };
    let model_file = crate::models::model_path(&w.cx.dirs.models, j.model);
    let total = cp.wav_ms.max(1);
    // 2. Each part not done yet.
    while cp.done < cp.chunks.len() {
        if w.should_stop() {
            return stopped(w);
        }
        let (from, to) = cp.chunks[cp.done];
        phase(w, "transcribe", Some(from * 100 / total), true);
        let part = j.dir.join("part.wav");
        let written = wav.read(from, to).and_then(|pcm| write_wav(&part, &pcm));
        if written.is_err() {
            return fail(w, ErrorCode::PathNotWritable, None);
        }
        // With `auto`, the first part's language holds for the rest so the
        // transcript has one language.
        let asked = match (j.language, cp.language.as_deref()) {
            ("auto", Some(l)) if l != "und" => l,
            (l, _) => l,
        };
        let out_base = j.dir.join("out");
        let _ = std::fs::remove_file(out_base.with_extension("json"));
        let args = whisper_args(
            &model_file,
            &part,
            &out_base,
            asked,
            j.threads,
            dtw_preset(&j.model.spec.id),
        );
        let exit = match run_tool(w, &j.whisper.exe, args, j.dir, |l| {
            if let Some(n) = parse_progress(l) {
                let pct = (from + (to - from) * n / 100) * 100 / total;
                phase(w, "transcribe", Some(pct.min(100)), n == 100);
            }
        }) {
            Ok(e) => e,
            Err(end) => return end,
        };
        match exit {
            Exit::Code(0) => {}
            Exit::Code(WIN_DLL_NOT_FOUND) => {
                return fail(
                    w,
                    ErrorCode::ToolMissing,
                    Some("whisper-cli could not load a library (Visual C++ runtime?)"),
                );
            }
            Exit::Code(n) => {
                return fail(
                    w,
                    ErrorCode::InternalError,
                    Some(&format!("whisper-cli exited {n}")),
                );
            }
            _ => return fail(w, ErrorCode::InternalError, Some("whisper-cli was killed")),
        }
        let json = out_base.with_extension("json");
        let raw = match std::fs::metadata(&json) {
            Ok(m) if m.len() <= MAX_OUTPUT_BYTES => std::fs::read(&json).ok(),
            _ => None,
        };
        let Some(raw) = raw else {
            return fail(
                w,
                ErrorCode::InternalError,
                Some("transcriber wrote no result"),
            );
        };
        let out = match parse_output(&raw, asked) {
            Ok(o) => o,
            Err(e) => return fail(w, ErrorCode::InternalError, Some(&e)),
        };
        // Checked against the part's own length (ends just past it are cut
        // to it), then moved to the recording's time.
        let segs = match transcripts::validate(&out.segments, Some(to - from)) {
            Ok(s) => s,
            Err(e) => return fail(w, ErrorCode::InternalError, Some(&e.to_string())),
        };
        if cp.language.is_none() {
            cp.language = Some(out.language);
        }
        cp.segments.extend(
            segs.into_iter()
                .map(|s| (s.start_ms + from, s.end_ms + from, s.text)),
        );
        cp.done += 1;
        let _ = std::fs::remove_file(&part);
        let _ = std::fs::remove_file(&json);
        if cp.done < cp.chunks.len() {
            if let Err(end) = save_part(w, j, &cp) {
                return end;
            }
        }
    }
    // 3. Check and save.
    phase(w, "transcribe_save", None, true);
    let segs: Vec<NewSegment> = cp
        .segments
        .iter()
        .map(|(a, b, t)| NewSegment::new(*a, *b, t.clone()))
        .collect();
    if segs.is_empty() {
        finish(w, j, None);
        return End::Ended;
    }
    let segs = match transcripts::validate(&segs, Some(cp.wav_ms)) {
        Ok(s) => s,
        Err(e) => return fail(w, ErrorCode::InternalError, Some(&e.to_string())),
    };
    finish(w, j, Some((origin(j, &cp), segs, cp.wav_ms)));
    End::Ended
}

fn origin(j: &Job<'_>, cp: &Checkpoint) -> Origin {
    Origin {
        item_id: j.item.id.clone(),
        asset_id: Some(j.asset.id.clone()),
        language: cp
            .language
            .clone()
            .unwrap_or_else(|| language_tag(Some(j.language))),
        provenance: Provenance::GeneratedLocal,
        provider: "whisper_cpp".into(),
        model_id: Some(j.model.spec.id.clone()),
        model_version: Some(j.model.spec.version.clone()),
        source_fingerprint: fingerprint(j),
        caption_kind: None,
    }
}

/// After a part that is not the last: the checkpoint file, then in one
/// transaction the job's checkpoint reference and the text so far as a
/// `partial` transcript (AC-23), unless a finished transcript of the same
/// model and language is there (a re-run never hides it behind a partial).
fn save_part<S: Store>(w: &Work<'_, S>, j: &Job<'_>, cp: &Checkpoint) -> Result<(), End> {
    if write_checkpoint(j.dir, cp).is_err() {
        return Err(fail(w, ErrorCode::PathNotWritable, None));
    }
    let o = origin(j, cp);
    let covered = cp.chunks[cp.done - 1].1;
    let segs: Vec<NewSegment> = cp
        .segments
        .iter()
        .map(|(a, b, t)| NewSegment::new(*a, *b, t.clone()))
        .collect();
    let cx = w.cx;
    let now = cx.now();
    let reference = format!("{}/{}", CHECKPOINT_FILE, cp.done);
    let r = cx.store.write(|c| {
        jobs::checkpoint_with(c, w.id(), w.generation(), Some(&reference), now, |tx| {
            let finished = transcripts::list_for_item(tx, &o.item_id)
                .map_err(|_| jobs::JobError::Invalid("transcript"))?
                .iter()
                .any(|t| {
                    t.status == Status::Ready
                        && t.origin.provenance == o.provenance
                        && t.origin.language == o.language
                        && t.origin.model_id == o.model_id
                });
            if !segs.is_empty() && !finished {
                transcripts::save(
                    tx,
                    &o,
                    Status::Partial,
                    Some(covered),
                    &segs,
                    Some(cp.wav_ms),
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
        Ok(()) => {
            cx.events.item_changed(&o.item_id);
            Ok(())
        }
        Err(jobs::JobError::Stale) => Err(End::Lost),
        Err(_) => Err(fail(
            w,
            ErrorCode::InternalError,
            Some("checkpoint not saved"),
        )),
    }
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

    /// `ms` of a loud tone with silence over `quiet` (ms spans).
    fn pcm(ms: i64, quiet: &[(i64, i64)]) -> Vec<u8> {
        (0..ms * 16)
            .flat_map(|i| {
                let t = i / 16;
                let silent = quiet.iter().any(|&(a, b)| t >= a && t < b);
                let v: i16 = if silent {
                    0
                } else if i % 2 == 0 {
                    8000
                } else {
                    -8000
                };
                v.to_le_bytes()
            })
            .collect()
    }

    fn plan(audio: &[u8], chunk: i64) -> Vec<(i64, i64)> {
        let total = audio.len() as i64 / BYTES_PER_MS;
        plan_chunks(total, chunk, |a, b| {
            let (a, b) = (a.clamp(0, total), b.clamp(0, total));
            Some(audio[(a * BYTES_PER_MS) as usize..(b * BYTES_PER_MS) as usize].to_vec())
        })
    }

    #[test]
    fn short_recordings_are_one_part() {
        assert_eq!(plan_chunks(0, 1000, |_, _| None), vec![(0, 0)]);
        assert_eq!(plan_chunks(1500, 1000, |_, _| None), vec![(0, 1500)]);
        assert_eq!(plan_chunks(9000, 0, |_, _| None), vec![(0, 9000)]);
    }

    #[test]
    fn parts_touch_cover_everything_and_cut_at_a_pause() {
        // 40 s, parts of 10 s: even cuts at 10, 20, 30 s; pauses near two.
        let audio = pcm(40_000, &[(11_000, 12_000), (28_500, 29_000)]);
        let p = plan(&audio, 10_000);
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].0, 0);
        assert_eq!(p.last().unwrap().1, 40_000);
        for w in p.windows(2) {
            assert_eq!(w[0].1, w[1].0, "parts touch, no overlap: {p:?}");
        }
        // Within 2.5 s (a quarter part) of the even cut, at the pause.
        assert!((11_000..12_000).contains(&p[0].1), "{p:?}");
        assert_eq!(p[1].1, 20_000, "no pause near: the even cut");
        assert!((28_500..29_000).contains(&p[2].1), "{p:?}");
        // Without audio to look at, the even cuts.
        assert_eq!(
            plan_chunks(40_000, 10_000, |_, _| None),
            vec![
                (0, 10_000),
                (10_000, 20_000),
                (20_000, 30_000),
                (30_000, 40_000)
            ]
        );
    }

    #[test]
    fn a_written_part_reads_back() {
        let dir = std::env::temp_dir().join(format!("vv-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.wav");
        let audio = pcm(2_000, &[(500, 1_000)]);
        write_wav(&path, &audio).unwrap();
        let mut w = Wav::open(&path).unwrap();
        assert_eq!(w.ms, 2_000);
        assert_eq!(w.read(500, 1_000).unwrap(), audio[16_000..32_000].to_vec());
        assert_eq!(w.read(1_900, 9_999).unwrap().len(), 100 * 32);
        assert_eq!(wav_duration_ms(&std::fs::read(&path).unwrap()), Some(2_000));
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
        let a = whisper_args(&p("m.bin"), &p("audio.wav"), &p("out"), "tr", 4, None);
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
        assert!((1..=8).contains(&threads(CpuUse::Full)));
        assert!(threads(CpuUse::Light) <= 2);
        assert!(!s.contains(&"-dtw".to_string()));

        let d = whisper_args(
            &p("m"),
            &p("a"),
            &p("o"),
            "en",
            2,
            dtw_preset("whisper-medium"),
        );
        let d: Vec<String> = d.into_iter().map(|x| x.into_string().unwrap()).collect();
        assert!(d.windows(2).any(|w| w == ["-dtw", "medium"]));
        assert!(d.contains(&"-ojf".to_string()) && d.contains(&"-nfa".to_string()));
        assert_eq!(d[d.len() - 2..], ["-of".to_string(), format!("{base}o")]);
        assert_eq!(dtw_preset("whisper-large-v3"), None);
    }

    fn whisper_json(segments: &str) -> Vec<u8> {
        format!(r#"{{"result": {{"language": "en"}}, "transcription": [{segments}]}}"#).into_bytes()
    }

    fn seg(from: i64, to: i64, text: &str, first_word_cs: Option<f64>) -> String {
        let tokens = match first_word_cs {
            None => String::new(),
            Some(t) => format!(
                r#", "tokens": [{{"text": "[_BEG_]", "t_dtw": -1}}, {{"text": ",", "t_dtw": 1}}, {{"text": " {text}", "t_dtw": {t}}}]"#
            ),
        };
        format!(r#"{{"offsets": {{"from": {from}, "to": {to}}}, "text": " {text}"{tokens}}}"#)
    }

    fn starts(raw: &[u8]) -> Vec<(i64, i64)> {
        parse_output(raw, "en")
            .unwrap()
            .segments
            .iter()
            .map(|s| (s.start_ms, s.end_ms))
            .collect()
    }

    #[test]
    fn segments_start_at_their_first_spoken_word() {
        // The intro: whisper says 0:00, the first word is at 5.02 s.
        let raw = whisper_json(
            &[
                seg(0, 7_000, "This", Some(502.0)),
                seg(7_000, 9_000, "is", Some(610.0)),
                seg(9_000, 12_000, "late", Some(1_020.0)),
            ]
            .join(","),
        );
        // Moved later; moved earlier, at most 1.5 s, and the line before
        // now ends where it begins.
        assert_eq!(
            starts(&raw),
            [(5_020, 6_100), (6_100, 9_000), (10_200, 12_000)]
        );
    }

    #[test]
    fn dtw_times_stay_inside_bounds() {
        let raw = whisper_json(
            &[
                // Past the end: kept 200 ms before it.
                seg(0, 1_000, "a", Some(500.0)),
                // Far earlier than whisper's start: at most 1.5 s earlier, and
                // not before the previous line's start.
                seg(10_000, 12_000, "b", Some(0.0)),
                // No DTW time, or a broken one: whisper's own start.
                seg(13_000, 14_000, "c", Some(-1.0)),
                seg(14_000, 15_000, "d", Some(1e300)),
                seg(15_000, 16_000, "e", None),
            ]
            .join(","),
        );
        assert_eq!(
            starts(&raw),
            [
                (800, 1_000),
                (8_500, 12_000),
                (13_000, 14_000),
                (14_000, 15_000),
                (15_000, 16_000)
            ]
        );
    }
}
