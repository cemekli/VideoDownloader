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
//! - **Merge target.** VP8/VP9/AV1 with Opus (or VP8/VP9 with Vorbis) go
//!   to WebM; H.264, HEVC, VP9 or AV1 with AAC, Opus or MP3 go to MP4. Only
//!   a pair neither holds goes to MKV: Chromium's Matroska reader does not
//!   take every pair the matrix files had, so MKV is the last resort.
//! - **Sound.** A codec yt-dlp does not report is not taken as "no audio"
//!   (X's plain MP4 links report none). A picture-only format is chosen
//!   only when a separate audio track can be merged with it, or nothing
//!   with sound exists.
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
    /// Whether the format may carry video / audio although its codec is not
    /// known. Absent in formats stored before this field: then the codec
    /// alone decides, as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<bool>,
}

impl StoredFormat {
    /// Formats stored before the flags: a codec named means it is there;
    /// a format with neither codec named (a direct file link) may have both,
    /// unless it is a storyboard.
    fn legacy_unknown(&self) -> bool {
        self.vcodec.is_none()
            && self.acodec.is_none()
            && self.ext.as_deref() != Some("mhtml")
            && self.protocol.as_deref() != Some("mhtml")
    }
    fn has_video(&self) -> bool {
        self.video
            .unwrap_or(self.vcodec.is_some() || self.legacy_unknown())
    }
    fn has_audio(&self) -> bool {
        self.audio
            .unwrap_or(self.acodec.is_some() || self.legacy_unknown())
    }
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
            video: Some(f.has_video),
            audio: Some(f.has_audio),
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
    /// Video is there but its codec is not reported.
    Unknown,
    Vp8,
    Av1,
    Vp9,
    H264,
}

fn family_of(f: &StoredFormat) -> VideoFamily {
    f.vcodec
        .as_deref()
        .map_or(VideoFamily::Unknown, video_family)
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
    use AudioFamily as A;
    use VideoFamily as V;
    match (v, a) {
        (V::Vp9 | V::Av1 | V::Vp8, A::Opus) | (V::Vp8 | V::Vp9, A::Vorbis) => "webm",
        (V::H264 | V::Hevc | V::Vp9 | V::Av1 | V::Unknown, A::Aac | A::Opus | A::Mp3) => "mp4",
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
        .filter(|f| usable(f) && !f.has_video() && f.has_audio())
        .collect();
    audio.sort_by_key(|f| {
        let fam = f.acodec.as_deref().map(audio_family);
        (
            prefer.is_some() && fam == prefer,
            fam.is_some(),
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
    let mut videos: Vec<&StoredFormat> = formats
        .iter()
        .filter(|f| usable(f) && f.has_video())
        .filter(fits)
        .collect();
    // Without a separate audio track, a picture-only format would download
    // silent: keep the formats with sound when there are any.
    if best_audio(formats, None).is_none() && videos.iter().any(|f| f.has_audio()) {
        videos.retain(|f| f.has_audio());
    }
    // Playable codecs first; within them the highest picture, then the
    // family the matrix and the merge targets like best, then bitrate.
    let pick = videos.iter().copied().max_by_key(|f| {
        let fam = family_of(f);
        (plays(fam), score(f).0, fam, f.has_audio(), score(f).1)
    })?;
    if pick.has_audio() {
        return Some(Choice {
            video: Some(pick),
            audio: None,
        });
    }
    let fam = family_of(pick);
    let prefer = match fam {
        VideoFamily::H264 | VideoFamily::Hevc | VideoFamily::Unknown => AudioFamily::Aac,
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
    let vfam = choice.video.map(family_of);
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
            video: None,
            audio: None,
        }
    }

    /// A format as yt-dlp reports it now, with presence flags.
    fn g(
        id: &str,
        ext: &str,
        v: (bool, Option<&str>),
        a: (bool, Option<&str>),
        h: Option<u32>,
    ) -> StoredFormat {
        StoredFormat {
            ext: Some(ext.into()),
            vcodec: v.1.map(Into::into),
            acodec: a.1.map(Into::into),
            video: Some(v.0),
            audio: Some(a.0),
            ..f(id, None, None, h, None)
        }
    }

    #[test]
    fn audio_with_an_unreported_codec_is_not_dropped() {
        // X: HLS video without sound, an HLS audio track whose codec is not
        // reported, and plain MP4 links that report no codec at all.
        let x = vec![
            g(
                "hls-2176",
                "mp4",
                (true, Some("avc1.640020")),
                (false, None),
                Some(720),
            ),
            g(
                "hls-audio-128000-Audio",
                "mp4",
                (false, None),
                (true, None),
                None,
            ),
            g("http-2176", "mp4", (true, None), (true, None), Some(720)),
        ];
        let (best, plan) = preset(&x, PresetId::Best);
        assert!(best.needs_merge, "{best:?}");
        assert_eq!(best.container.as_deref(), Some("mp4"));
        assert_eq!(
            plan.unwrap().selector,
            "hls-2176+hls-audio-128000-Audio/bv*+ba/b"
        );
        // Without the audio track, the link with sound wins over a silent
        // picture of the same height.
        let no_track = vec![x[0].clone(), x[2].clone()];
        let (best, plan) = preset(&no_track, PresetId::Best);
        assert!(!best.needs_merge);
        assert!(!best.may_not_play);
        assert_eq!(plan.unwrap().selector, "http-2176/bv*+ba/b");
        // Stored before the flags: a direct file link with no codec named
        // (Dailymotion's `..._mp4_h264_aac_hq`) is offered, a storyboard not.
        let mut old = x[2].clone();
        old.video = None;
        old.audio = None;
        assert!(preset(&[old.clone()], PresetId::Best).0.available);
        old.ext = Some("mhtml".into());
        assert!(!preset(&[old], PresetId::Best).0.available);
    }

    #[test]
    fn mkv_only_when_neither_mp4_nor_webm_holds_the_pair() {
        let pair = |v: &str, a: &str| {
            let fs = vec![
                f("v", Some(v), None, Some(720), None),
                f("a", None, Some(a), None, None),
            ];
            preset(&fs, PresetId::Best).0.container.unwrap()
        };
        assert_eq!(pair("vp09.00.31.08", "mp4a.40.2"), "mp4");
        assert_eq!(pair("av01.0.05M.08", "mp4a.40.5"), "mp4");
        assert_eq!(pair("avc1.64001f", "opus"), "mp4");
        assert_eq!(pair("vp9", "opus"), "webm");
        assert_eq!(pair("vp8", "vorbis"), "webm");
        assert_eq!(pair("avc1.64001f", "vorbis"), "mkv");
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
