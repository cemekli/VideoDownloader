//! Quality presets from real format data (STATE.md F3.5; SPEC DL-01, DL-02,
//! §7.5 "Öneri"; ADR-0004, ADR-0021).
//!
//! The UI never sends a format selector or any yt-dlp option: it names a
//! preset (`best`, `1080`, `720`, `audio`) and the backend turns it into a
//! selector from the formats the metadata run stored. The rules:
//!
//! - **Playable target.** Video codecs that played in the WebView2 matrix
//!   (H.264, VP9, AV1, VP8) are preferred; HEVC played without a picture, so
//!   it is used only when nothing else exists, and then the preset says
//!   "may not play in the app". Nothing is transcoded.
//! - **Merge target.** H.264 with AAC goes to MP4; VP9/AV1 with Opus to
//!   WebM; any other pair to MKV (Matroska played in the matrix too).
//! - **Honest size.** A size is given only when every chosen format reports
//!   one; `filesize_approx` is shown as "about"; otherwise unknown (DL-02).
//! - The selector names the chosen format ids first and falls back to a
//!   generic expression with the same limits, for when the site renumbered
//!   its formats since the metadata run.

use serde::{Deserialize, Serialize};

/// What the metadata run keeps of one format: no URLs, no headers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredFormat {
    pub format_id: String,
    pub ext: Option<String>,
    pub protocol: Option<String>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub tbr: Option<f64>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub has_drm: bool,
}

impl From<&vv_ytdlp::metadata::Format> for StoredFormat {
    fn from(f: &vv_ytdlp::metadata::Format) -> Self {
        StoredFormat {
            format_id: f.format_id.clone(),
            ext: f.ext.clone(),
            protocol: f.protocol.clone(),
            vcodec: f.vcodec.clone(),
            acodec: f.acodec.clone(),
            width: f.width,
            height: f.height,
            fps: f.fps,
            tbr: f.tbr,
            filesize: f.filesize,
            filesize_approx: f.filesize_approx,
            has_drm: f.has_drm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetId {
    Best,
    #[serde(rename = "1080")]
    Upto1080,
    #[serde(rename = "720")]
    Upto720,
    Audio,
}

impl PresetId {
    pub const ALL: [PresetId; 4] = [
        PresetId::Best,
        PresetId::Upto1080,
        PresetId::Upto720,
        PresetId::Audio,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PresetId::Best => "best",
            PresetId::Upto1080 => "1080",
            PresetId::Upto720 => "720",
            PresetId::Audio => "audio",
        }
    }

    pub fn parse(s: &str) -> Option<PresetId> {
        PresetId::ALL.into_iter().find(|p| p.as_str() == s)
    }

    fn max_height(self) -> Option<u32> {
        match self {
            PresetId::Upto1080 => Some(1080),
            PresetId::Upto720 => Some(720),
            _ => None,
        }
    }
}

/// One preset as the Download dialog shows it. Internal parts (the
/// selector) stay in [`Plan`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: PresetId,
    pub available: bool,
    /// `mp4`, `webm`, `mkv`, `m4a`, ...; `None` when no format data exists.
    pub container: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub height: Option<u32>,
    /// Exact total, only when every chosen format reports its size.
    pub size_bytes: Option<u64>,
    /// yt-dlp's estimate, shown as "about"; never as an exact value.
    pub size_estimate_bytes: Option<u64>,
    /// Only HEVC (or an unknown codec) was available: the file is kept, but
    /// the in-app player may show no picture (ADR-0004).
    pub may_not_play: bool,
    /// Separate video and audio are merged after the download.
    pub needs_merge: bool,
}

/// What the download handler runs for a preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub selector: String,
    pub merge_format: Option<String>,
}

fn norm(c: &str) -> String {
    c.to_ascii_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum VideoFamily {
    Hevc,
    Other,
    Vp8,
    Av1,
    Vp9,
    H264,
}

fn video_family(c: &str) -> VideoFamily {
    let c = norm(c);
    if c.starts_with("avc") || c.starts_with("h264") {
        VideoFamily::H264
    } else if c.starts_with("vp09") || c.starts_with("vp9") {
        VideoFamily::Vp9
    } else if c.starts_with("av01") || c.starts_with("av1") {
        VideoFamily::Av1
    } else if c.starts_with("vp8") {
        VideoFamily::Vp8
    } else if c.starts_with("hvc") || c.starts_with("hev") || c.starts_with("h265") {
        VideoFamily::Hevc
    } else {
        VideoFamily::Other
    }
}

fn plays(v: VideoFamily) -> bool {
    !matches!(v, VideoFamily::Hevc | VideoFamily::Other)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioFamily {
    Aac,
    Opus,
    Vorbis,
    Mp3,
    Other,
}

fn audio_family(c: &str) -> AudioFamily {
    let c = norm(c);
    if c.starts_with("mp4a") || c.starts_with("aac") {
        AudioFamily::Aac
    } else if c.starts_with("opus") {
        AudioFamily::Opus
    } else if c.starts_with("vorbis") {
        AudioFamily::Vorbis
    } else if c.starts_with("mp3") {
        AudioFamily::Mp3
    } else {
        AudioFamily::Other
    }
}

/// Where separate streams are merged to.
fn merge_target(v: VideoFamily, a: AudioFamily) -> &'static str {
    match (v, a) {
        (VideoFamily::H264, AudioFamily::Aac) => "mp4",
        (VideoFamily::Vp9 | VideoFamily::Av1 | VideoFamily::Vp8, AudioFamily::Opus)
        | (VideoFamily::Vp8 | VideoFamily::Vp9, AudioFamily::Vorbis) => "webm",
        _ => "mkv",
    }
}

fn usable(f: &StoredFormat) -> bool {
    !f.has_drm
        && !matches!(
            f.protocol.as_deref(),
            Some("rtmp" | "rtmpe" | "rtsp" | "mms" | "f4m" | "ism")
        )
        && !f.format_id.contains(['+', '/', ',', '[', ']', ' '])
}

fn score(f: &StoredFormat) -> (u32, u64) {
    (
        f.height.unwrap_or(0),
        f.tbr.map_or(0, |t| (t * 1000.0) as u64),
    )
}

fn size(fs: &[&StoredFormat]) -> (Option<u64>, Option<u64>) {
    let exact: Option<u64> = fs.iter().map(|f| f.filesize).sum();
    let approx: Option<u64> = fs.iter().map(|f| f.filesize.or(f.filesize_approx)).sum();
    (exact, if exact.is_some() { None } else { approx })
}

struct Choice<'a> {
    video: Option<&'a StoredFormat>,
    audio: Option<&'a StoredFormat>,
}

fn best_audio(formats: &[StoredFormat], prefer: Option<AudioFamily>) -> Option<&StoredFormat> {
    let mut audio: Vec<&StoredFormat> = formats
        .iter()
        .filter(|f| usable(f) && f.vcodec.is_none() && f.acodec.is_some())
        .collect();
    audio.sort_by_key(|f| {
        let fam = f.acodec.as_deref().map(audio_family);
        (
            prefer.is_some() && fam == prefer,
            f.tbr.map_or(0, |t| (t * 1000.0) as u64),
        )
    });
    audio.last().copied()
}

fn choose(formats: &[StoredFormat], preset: PresetId) -> Option<Choice<'_>> {
    if preset == PresetId::Audio {
        return best_audio(formats, Some(AudioFamily::Aac)).map(|a| Choice {
            video: None,
            audio: Some(a),
        });
    }
    let max = preset.max_height();
    let fits = |f: &&StoredFormat| match (max, f.height) {
        (Some(m), Some(h)) => h <= m,
        (Some(_), None) => false,
        (None, _) => true,
    };
    let videos: Vec<&StoredFormat> = formats
        .iter()
        .filter(|f| usable(f) && f.vcodec.is_some())
        .filter(fits)
        .collect();
    // Playable codecs first; within them the highest picture, then the
    // family the matrix and the merge targets like best, then bitrate.
    let pick = videos.iter().copied().max_by_key(|f| {
        let fam = video_family(f.vcodec.as_deref().unwrap_or(""));
        (plays(fam), score(f).0, fam, f.acodec.is_some(), score(f).1)
    })?;
    if pick.acodec.is_some() {
        return Some(Choice {
            video: Some(pick),
            audio: None,
        });
    }
    let fam = video_family(pick.vcodec.as_deref().unwrap_or(""));
    let prefer = match fam {
        VideoFamily::H264 => AudioFamily::Aac,
        _ => AudioFamily::Opus,
    };
    Some(Choice {
        video: Some(pick),
        audio: best_audio(formats, Some(prefer)),
    })
}

fn fallback(preset: PresetId) -> String {
    match preset.max_height() {
        Some(h) => format!("bv*[height<={h}]+ba/b[height<={h}]"),
        None if preset == PresetId::Audio => "ba/b".to_owned(),
        None => "bv*+ba/b".to_owned(),
    }
}

/// Every preset for this format list, with availability. An empty list
/// (a direct file link) offers only `best`, with unknown size.
pub fn presets(formats: &[StoredFormat]) -> Vec<(Preset, Option<Plan>)> {
    PresetId::ALL
        .into_iter()
        .map(|id| preset(formats, id))
        .collect()
}

pub fn preset(formats: &[StoredFormat], id: PresetId) -> (Preset, Option<Plan>) {
    let empty = Preset {
        id,
        available: false,
        container: None,
        video_codec: None,
        audio_codec: None,
        height: None,
        size_bytes: None,
        size_estimate_bytes: None,
        may_not_play: false,
        needs_merge: false,
    };
    if formats.is_empty() {
        return if id == PresetId::Best {
            (
                Preset {
                    available: true,
                    ..empty
                },
                Some(Plan {
                    selector: fallback(id),
                    merge_format: None,
                }),
            )
        } else {
            (empty, None)
        };
    }
    let Some(choice) = choose(formats, id) else {
        return (empty, None);
    };
    let chosen: Vec<&StoredFormat> = choice
        .video
        .iter()
        .chain(choice.audio.iter())
        .copied()
        .collect();
    let (exact, approx) = size(&chosen);
    let vfam = choice
        .video
        .and_then(|v| v.vcodec.as_deref())
        .map(video_family);
    let afam = choice
        .audio
        .or(choice.video)
        .and_then(|f| f.acodec.as_deref())
        .map(audio_family);
    let needs_merge = choice.video.is_some() && choice.audio.is_some();
    let container = match (vfam, afam, needs_merge) {
        (Some(v), Some(a), true) => Some(merge_target(v, a).to_owned()),
        _ => chosen.first().and_then(|f| f.ext.clone()),
    };
    let ids: Vec<&str> = chosen.iter().map(|f| f.format_id.as_str()).collect();
    let selector = format!("{}/{}", ids.join("+"), fallback(id));
    (
        Preset {
            id,
            available: true,
            container: container.clone(),
            video_codec: choice.video.and_then(|v| v.vcodec.clone()),
            audio_codec: choice.audio.or(choice.video).and_then(|f| f.acodec.clone()),
            height: choice.video.and_then(|v| v.height),
            size_bytes: exact,
            size_estimate_bytes: approx,
            may_not_play: vfam.is_some_and(|v| !plays(v)),
            needs_merge,
        },
        Some(Plan {
            selector,
            merge_format: if needs_merge { container } else { None },
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(
        id: &str,
        v: Option<&str>,
        a: Option<&str>,
        h: Option<u32>,
        size: Option<u64>,
    ) -> StoredFormat {
        StoredFormat {
            format_id: id.into(),
            ext: Some(match (v, a) {
                (Some(v), _) if v.starts_with("avc") => "mp4".into(),
                (Some(_), _) => "webm".into(),
                (None, Some(a)) if a.starts_with("mp4a") => "m4a".into(),
                _ => "webm".into(),
            }),
            protocol: Some("https".into()),
            vcodec: v.map(Into::into),
            acodec: a.map(Into::into),
            width: None,
            height: h,
            fps: None,
            tbr: h.map(|h| h as f64),
            filesize: size,
            filesize_approx: None,
            has_drm: false,
        }
    }

    fn youtube_like() -> Vec<StoredFormat> {
        vec![
            f(
                "18",
                Some("avc1.42001E"),
                Some("mp4a.40.2"),
                Some(360),
                Some(10),
            ),
            f("137", Some("avc1.640028"), None, Some(1080), Some(1000)),
            f("248", Some("vp9"), None, Some(1080), Some(900)),
            f("136", Some("avc1.4d401f"), None, Some(720), Some(500)),
            f("313", Some("vp9"), None, Some(2160), Some(5000)),
            f("140", None, Some("mp4a.40.2"), None, Some(100)),
            f("251", None, Some("opus"), None, Some(90)),
        ]
    }

    #[test]
    fn presets_follow_the_playback_matrix_and_merge_targets() {
        let fs = youtube_like();
        let (best, plan) = preset(&fs, PresetId::Best);
        assert_eq!(best.height, Some(2160));
        assert_eq!(best.container.as_deref(), Some("webm"));
        assert_eq!(plan.unwrap().selector, "313+251/bv*+ba/b");
        let (p1080, plan) = preset(&fs, PresetId::Upto1080);
        // At equal height H.264 wins: MP4 with AAC.
        assert_eq!(p1080.video_codec.as_deref(), Some("avc1.640028"));
        assert_eq!(p1080.container.as_deref(), Some("mp4"));
        assert_eq!(p1080.size_bytes, Some(1100));
        assert!(p1080.needs_merge);
        let plan = plan.unwrap();
        assert_eq!(
            plan.selector,
            "137+140/bv*[height<=1080]+ba/b[height<=1080]"
        );
        assert_eq!(plan.merge_format.as_deref(), Some("mp4"));
        let (p720, _) = preset(&fs, PresetId::Upto720);
        assert_eq!(p720.height, Some(720));
        let (audio, plan) = preset(&fs, PresetId::Audio);
        assert_eq!(audio.container.as_deref(), Some("m4a"));
        assert!(!audio.needs_merge);
        assert_eq!(plan.unwrap().selector, "140/ba/b");
    }

    #[test]
    fn unknown_sizes_are_not_invented() {
        let mut fs = youtube_like();
        for x in &mut fs {
            x.filesize = None;
        }
        fs[1].filesize_approx = Some(1234);
        let (p, _) = preset(&fs, PresetId::Upto1080);
        assert_eq!(p.size_bytes, None);
        // The audio part has no estimate either, so there is no total.
        assert_eq!(p.size_estimate_bytes, None);
        fs[5].filesize_approx = Some(66);
        let (p, _) = preset(&fs, PresetId::Upto1080);
        assert_eq!(p.size_estimate_bytes, Some(1300));
    }

    #[test]
    fn hevc_only_is_offered_with_a_warning_and_other_codecs_win() {
        let fs = vec![
            f(
                "h1",
                Some("hvc1.1.6.L93"),
                Some("mp4a.40.2"),
                Some(1080),
                None,
            ),
            f(
                "a1",
                Some("avc1.64001f"),
                Some("mp4a.40.2"),
                Some(720),
                None,
            ),
        ];
        let (best, plan) = preset(&fs, PresetId::Best);
        assert_eq!(best.video_codec.as_deref(), Some("avc1.64001f"));
        assert!(!best.may_not_play);
        assert_eq!(plan.unwrap().selector, "a1/bv*+ba/b");
        let only = vec![fs[0].clone()];
        let (best, _) = preset(&only, PresetId::Best);
        assert!(best.may_not_play);
        assert!(!preset(&only, PresetId::Audio).0.available);
    }

    #[test]
    fn direct_links_get_only_best_and_drm_is_never_chosen() {
        let (best, plan) = preset(&[], PresetId::Best);
        assert!(best.available);
        assert_eq!(best.container, None);
        assert_eq!(plan.unwrap().selector, "bv*+ba/b");
        assert!(!preset(&[], PresetId::Upto720).0.available);
        let mut drm = f("x", Some("avc1"), Some("mp4a"), Some(720), None);
        drm.has_drm = true;
        assert!(!preset(&[drm], PresetId::Best).0.available);
    }

    #[test]
    fn hostile_format_ids_cannot_change_the_selector() {
        let fs = vec![f(
            "1/bestvideo",
            Some("avc1"),
            Some("mp4a"),
            Some(720),
            None,
        )];
        assert!(!preset(&fs, PresetId::Best).0.available);
    }

    #[test]
    fn preset_ids_round_trip() {
        for p in PresetId::ALL {
            assert_eq!(PresetId::parse(p.as_str()), Some(p));
            assert_eq!(serde_json::to_value(p).unwrap(), p.as_str());
        }
        assert_eq!(PresetId::parse("--exec"), None);
    }
}
