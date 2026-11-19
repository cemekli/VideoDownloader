//! Fixed FFmpeg/ffprobe flags (SPEC §14.1).
//!
//! Mirrors `tests/fixtures/ffmpeg/args.json`, which `scripts/ffmpeg-probe.mjs`
//! runs against the pinned build in CI; a test fails if the two drift.
//!
//! Two allowlists, because one is not enough (ADR-0006):
//! - `-protocol_whitelist file` stops network protocols, but an HLS playlist
//!   or concat list may still name *any local file* through `file`;
//! - `-format_whitelist` stops those demuxers from being chosen at all, so a
//!   manifest is rejected before it can open what it points to.
//!
//! Paths are passed as `file:<absolute path>`, one argv entry each, so a name
//! like `concat:a|b` or `http:x` is never read as a protocol.

use std::ffi::OsString;
use std::path::Path;

/// Demuxers a local import may use: the containers of IMP-02 and the audio
/// formats VideoVault accepts. None of them follows references to other files
/// or URLs (the mov demuxer's external data references stay off by default,
/// `enable_drefs=0`).
pub const FORMAT_WHITELIST: &str = "mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,mp3,ogg,flac,wav,aac";

/// Every run: no banner, errors only.
pub const COMMON: &[&str] = &["-hide_banner", "-loglevel", "error"];

/// Input side, before `-i`. The same for ffprobe and ffmpeg.
pub const INPUT: &[&str] = &[
    "-protocol_whitelist",
    "file",
    "-format_whitelist",
    FORMAT_WHITELIST,
];

/// ffprobe output: only the fields [`crate::probe`] reads, as JSON.
pub const PROBE: &[&str] = &[
    "-show_entries",
    "format=format_name,duration,size:stream=index,codec_type,codec_name,width,height,channels:stream_tags=language",
    "-of",
    "json",
];

/// MKV/WebM -> MP4 remux without re-encoding (PLAY-07, ADR-0004/0006): first
/// video and, if present, first audio stream. `-n` never overwrites; the
/// caller still writes to a fresh name and checks the result, because
/// FFmpeg 6.1 exits 0 when it refuses to overwrite.
pub const REMUX_OUTPUT: &[&str] = &[
    "-map",
    "0:v:0",
    "-map",
    "0:a:0?",
    "-c",
    "copy",
    "-movflags",
    "+faststart",
    "-protocol_whitelist",
    "file",
    "-f",
    "mp4",
    "-n",
];

/// One JPEG frame from the first video stream, 320 px wide (IMP-06). Seeking
/// happens before `-i` (fast, keyframe based); `-n` never overwrites.
pub const THUMBNAIL_OUTPUT: &[&str] = &[
    "-map",
    "0:v:0",
    "-frames:v",
    "1",
    "-vf",
    "scale=320:-2",
    "-c:v",
    "mjpeg",
    "-q:v",
    "5",
    "-protocol_whitelist",
    "file",
    "-f",
    "image2",
    "-update",
    "1",
    "-n",
];

/// The first audio stream as 16 kHz mono 16-bit WAV, what whisper.cpp reads
/// (F6.2, ADR-0028). Video, subtitles and data streams are dropped; `-n`
/// never overwrites.
pub const AUDIO_WAV_OUTPUT: &[&str] = &[
    "-map",
    "0:a:0",
    "-vn",
    "-sn",
    "-dn",
    "-ac",
    "1",
    "-ar",
    "16000",
    "-c:a",
    "pcm_s16le",
    "-protocol_whitelist",
    "file",
    "-f",
    "wav",
    "-n",
];

/// Still frames of the first video stream as PNG, for reading the text on
/// screen (F7.4, AI-C01, ADR-0039). Which frames is [`frames_args`]'s
/// filter; audio, subtitles and data are dropped, a frame is written only
/// when the filter passes one (`vfr`), and `-n` never overwrites.
pub const FRAMES_OUTPUT: &[&str] = &[
    "-map",
    "0:v:0",
    "-an",
    "-sn",
    "-dn",
    "-fps_mode",
    "vfr",
    "-c:v",
    "png",
    "-protocol_whitelist",
    "file",
    "-f",
    "image2",
    "-n",
];

/// The times of the written frames, one block per frame, relative to the
/// working directory of the run (a path in a filter needs escaping).
pub const FRAME_LIST: &str = "frames.txt";

/// File names of the frames in their folder: `f-00001.png`, … in order.
pub const FRAME_PATTERN: &str = "f-%05d.png";

/// Which frames [`frames_args`] keeps: never every frame (AI-C01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePick {
    /// A frame at least this often (sparse sampling).
    pub every_ms: u64,
    /// A scene change gives a frame when the last one is this far back.
    pub scene_gap_ms: u64,
    /// Scene change threshold in thousandths (300 = 0.3).
    pub scene_milli: u32,
    /// The run stops after this many frames (AI-C02 budget).
    pub max_frames: u32,
    /// Wider frames are scaled down to this width.
    pub max_width: u32,
    /// Decode only keyframes: fast, and most cuts start one. A file with
    /// few keyframes (a screen recording) needs a full pass.
    pub keyframes_only: bool,
}

fn secs(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// The `-vf` filter of [`frames_args`].
pub fn frame_filter(p: &FramePick) -> String {
    // A full pass looks at four frames a second: cheaper scene scores, and
    // text on screen stays longer than a quarter second.
    let thin = if p.keyframes_only { "" } else { "fps=4," };
    format!(
        "{thin}select='isnan(prev_selected_t)+gte(t-prev_selected_t\\,{every})+gt(scene\\,{scene})*gte(t-prev_selected_t\\,{gap})',\
         scale='min(iw\\,{w})':-2,metadata=mode=print:file={FRAME_LIST}",
        every = secs(p.every_ms),
        scene = secs(u64::from(p.scene_milli)),
        gap = secs(p.scene_gap_ms),
        w = p.max_width,
    )
}

/// Full ffmpeg argv for the frames of one local file, written as
/// [`FRAME_PATTERN`] into `dir`. Run it with `dir` as the working directory:
/// the frame list goes there.
pub fn frames_args(input: &Path, dir: &Path, p: &FramePick) -> Result<Vec<OsString>, ArgsError> {
    absolute(input)?;
    absolute(dir)?;
    let mut v: Vec<OsString> = strs(COMMON).collect();
    v.push("-nostdin".into());
    v.extend(strs(INPUT));
    if p.keyframes_only {
        v.push("-skip_frame".into());
        v.push("nokey".into());
    }
    v.push("-i".into());
    v.push(file_arg(input));
    v.push("-vf".into());
    v.push(frame_filter(p).into());
    v.push("-frames:v".into());
    v.push(p.max_frames.to_string().into());
    v.extend(strs(FRAMES_OUTPUT));
    v.push(file_arg(&dir.join(FRAME_PATTERN)));
    Ok(v)
}

/// Frame times in ms from the frame list, in written order (frame `k` is
/// file `k + 1`). Lines other than `frame:… pts_time:…` are skipped.
pub fn parse_frame_list(text: &str) -> Vec<i64> {
    text.lines()
        .filter(|l| l.starts_with("frame:"))
        .filter_map(|l| {
            let t = l
                .split_whitespace()
                .find_map(|w| w.strip_prefix("pts_time:"))?;
            let s: f64 = t.parse().ok()?;
            (s.is_finite() && s >= 0.0).then(|| (s * 1000.0).round() as i64)
        })
        .collect()
}

/// `file:` + the path, one argv entry.
pub fn file_arg(path: &Path) -> OsString {
    let mut v = OsString::from("file:");
    v.push(path.as_os_str());
    v
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgsError {
    /// Relative paths depend on the working directory; the caller resolves
    /// and validates paths first (SPEC §20.1).
    NotAbsolute,
}

fn absolute(p: &Path) -> Result<(), ArgsError> {
    if p.is_absolute() {
        Ok(())
    } else {
        Err(ArgsError::NotAbsolute)
    }
}

fn strs<'a>(a: &'a [&'a str]) -> impl Iterator<Item = OsString> + 'a {
    a.iter().map(OsString::from)
}

/// Full ffprobe argv (without the program) for one local file.
pub fn probe_args(input: &Path) -> Result<Vec<OsString>, ArgsError> {
    absolute(input)?;
    let mut v: Vec<OsString> = strs(COMMON).chain(strs(INPUT)).chain(strs(PROBE)).collect();
    v.push("-i".into());
    v.push(file_arg(input));
    Ok(v)
}

/// Full ffmpeg argv for a stream-copy remux to MP4.
pub fn remux_args(input: &Path, output: &Path) -> Result<Vec<OsString>, ArgsError> {
    absolute(input)?;
    absolute(output)?;
    let mut v: Vec<OsString> = strs(COMMON).collect();
    v.push("-nostdin".into());
    v.extend(strs(INPUT));
    v.push("-i".into());
    v.push(file_arg(input));
    v.extend(strs(REMUX_OUTPUT));
    v.push(file_arg(output));
    Ok(v)
}

/// Full ffmpeg argv for one thumbnail frame taken at `at_ms` into the video.
pub fn thumbnail_args(input: &Path, output: &Path, at_ms: u64) -> Result<Vec<OsString>, ArgsError> {
    absolute(input)?;
    absolute(output)?;
    let mut v: Vec<OsString> = strs(COMMON).collect();
    v.push("-nostdin".into());
    v.extend(strs(INPUT));
    v.push("-ss".into());
    v.push(format!("{}.{:03}", at_ms / 1000, at_ms % 1000).into());
    v.push("-i".into());
    v.push(file_arg(input));
    v.extend(strs(THUMBNAIL_OUTPUT));
    v.push(file_arg(output));
    Ok(v)
}

/// Full ffmpeg argv for the transcription audio of one local file.
pub fn audio_wav_args(input: &Path, output: &Path) -> Result<Vec<OsString>, ArgsError> {
    absolute(input)?;
    absolute(output)?;
    let mut v: Vec<OsString> = strs(COMMON).collect();
    v.push("-nostdin".into());
    v.extend(strs(INPUT));
    v.push("-i".into());
    v.push(file_arg(input));
    v.extend(strs(AUDIO_WAV_OUTPUT));
    v.push(file_arg(output));
    Ok(v)
}
