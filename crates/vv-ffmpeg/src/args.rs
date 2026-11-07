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
