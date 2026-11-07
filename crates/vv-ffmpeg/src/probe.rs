//! What the restricted ffprobe says about a local file (IMP-02, AC-59).
//!
//! Two outcomes reach the UI as errors:
//! - `INPUT_REJECTED`: the file is a playlist, manifest or list that points to
//!   other files or URLs (HLS, DASH, concat, ...). ffprobe refuses it under
//!   [`crate::args::FORMAT_WHITELIST`] and names the demuxer it detected.
//! - `MEDIA_CORRUPT`: anything else that is not usable media: an unknown or
//!   unsupported format, damaged data, no audio or video stream.

use serde::Deserialize;
use vv_domain::error::ErrorCode;

/// Demuxers that read other files or URLs named inside the input.
pub const REFERENCING_DEMUXERS: &[&str] = &[
    "hls",
    "dash",
    "concat",
    "imf",
    "webm_dash_manifest",
    "sdp",
    "image2",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// ffprobe's `mov,mp4,m4a,3gp,3g2,mj2`: MP4, MOV, M4A.
    Mp4,
    /// `matroska,webm`: MKV and WebM (the demuxer cannot tell them apart).
    Matroska,
    Mp3,
    Ogg,
    Flac,
    Wav,
    Aac,
}

impl Container {
    /// Maps ffprobe's `format_name`. Only demuxers in the whitelist map.
    pub fn from_format_name(name: &str) -> Option<Container> {
        Some(match name {
            "mov,mp4,m4a,3gp,3g2,mj2" => Container::Mp4,
            "matroska,webm" => Container::Matroska,
            "mp3" => Container::Mp3,
            "ogg" => Container::Ogg,
            "flac" => Container::Flac,
            "wav" => Container::Wav,
            "aac" => Container::Aac,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProbeStream {
    pub index: u32,
    #[serde(default)]
    pub codec_type: Option<String>,
    #[serde(default)]
    pub codec_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProbeFormat {
    pub format_name: String,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
}

/// The JSON that [`crate::args::PROBE`] asks for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProbeReport {
    #[serde(default)]
    pub streams: Vec<ProbeStream>,
    pub format: ProbeFormat,
}

/// A local file that may be imported.
#[derive(Debug, Clone, PartialEq)]
pub struct Probed {
    pub container: Container,
    /// Codec of the first video stream (`h264`, `hevc`, `vp9`, `av1`, ...).
    pub video: Option<String>,
    /// Codec of the first audio stream (`aac`, `opus`, `vorbis`, ...).
    pub audio: Option<String>,
    pub duration_s: Option<f64>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// A playlist/manifest/list format; the demuxer ffprobe detected.
    Referencing { demuxer: String },
    /// A real format, but not one VideoVault imports.
    UnsupportedFormat { format: String },
    /// ffprobe failed or printed something we cannot read.
    Unreadable,
    /// No audio or video stream.
    NoMediaStreams,
}

impl ProbeError {
    pub fn code(&self) -> ErrorCode {
        match self {
            ProbeError::Referencing { .. } => ErrorCode::InputRejected,
            _ => ErrorCode::MediaCorrupt,
        }
    }
}

pub fn parse_probe_json(stdout: &str) -> Result<ProbeReport, ProbeError> {
    serde_json::from_str(stdout).map_err(|_| ProbeError::Unreadable)
}

/// Classifies a finished ffprobe run. `success` is its exit status.
pub fn classify(success: bool, stdout: &str, stderr: &str) -> Result<Probed, ProbeError> {
    if let Some(demuxer) = rejected_demuxer(stderr) {
        return Err(
            if REFERENCING_DEMUXERS.contains(&demuxer) || is_protocol(demuxer) {
                ProbeError::Referencing {
                    demuxer: demuxer.to_owned(),
                }
            } else {
                ProbeError::UnsupportedFormat {
                    format: demuxer.to_owned(),
                }
            },
        );
    }
    if !success {
        return Err(ProbeError::Unreadable);
    }
    let r = parse_probe_json(stdout)?;
    // Defence in depth: never trust a format the whitelist should have stopped.
    let container = Container::from_format_name(&r.format.format_name).ok_or_else(|| {
        let f = &r.format.format_name;
        if f.split(',').any(|n| REFERENCING_DEMUXERS.contains(&n)) {
            ProbeError::Referencing { demuxer: f.clone() }
        } else {
            ProbeError::UnsupportedFormat { format: f.clone() }
        }
    })?;
    let first = |kind: &str| {
        r.streams
            .iter()
            .find(|s| s.codec_type.as_deref() == Some(kind))
            .map(|s| s.codec_name.clone().unwrap_or_default())
    };
    let (video, audio) = (first("video"), first("audio"));
    if video.is_none() && audio.is_none() {
        return Err(ProbeError::NoMediaStreams);
    }
    Ok(Probed {
        container,
        video,
        audio,
        duration_s: r.format.duration.as_deref().and_then(|d| d.parse().ok()),
        size_bytes: r.format.size.as_deref().and_then(|s| s.parse().ok()),
    })
}

/// `[hls @ 0x5597716bd980] Format not on whitelist '...'` -> `hls`.
/// `[http @ 0x...] Protocol 'http' not on whitelist 'file'!` -> `http`: a
/// reference to a non-file protocol, which is a referencing input too.
pub fn rejected_demuxer(stderr: &str) -> Option<&str> {
    stderr.lines().find_map(|l| {
        let rest = l.trim().strip_prefix('[')?;
        let (ctx, msg) = rest.split_once(']')?;
        let msg = msg.trim_start();
        if !msg.starts_with("Format not on whitelist")
            && !(msg.starts_with("Protocol '") && msg.contains("' not on whitelist"))
        {
            return None;
        }
        let name = ctx.split(" @ ").next()?.trim();
        (!name.is_empty()).then_some(name)
    })
}

/// Protocols ffmpeg may name in a whitelist refusal; all of them reach
/// outside the given file.
fn is_protocol(name: &str) -> bool {
    matches!(
        name,
        "http"
            | "https"
            | "tcp"
            | "tls"
            | "udp"
            | "rtmp"
            | "rtp"
            | "rtsp"
            | "ftp"
            | "sftp"
            | "srt"
            | "rist"
            | "crypto"
            | "data"
            | "subfile"
            | "concat"
            | "concatf"
            | "pipe"
            | "async"
            | "cache"
            | "gopher"
            | "smb"
            | "zmq"
    )
}
