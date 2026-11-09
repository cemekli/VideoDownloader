//! Platform recognition and URL normalization (SPEC §6, §7.1, §11.7;
//! STATE.md F1.3, ADR-0011).
//!
//! Works offline on a URL that already passed [`crate::url_policy`]. For a
//! known platform the canonical URL is *rebuilt* from the content id, so
//! tracking and signature parameters cannot leak into it. For anything else
//! the URL is kept as given (only scheme/host case, default port and the
//! fragment are normalized): generic query parameters are never dropped
//! blindly (§7.1). Short links (`youtu.be` excepted) cannot be resolved
//! offline and stay generic-shaped until metadata (M2) resolves them.
//!
//! Every change to the output for some input bumps [`NORMALIZATION_VERSION`];
//! stored items with an older version are recomputed by
//! `vv_persistence::library::renormalize` (§11.7, AC-44).

use crate::url_policy::CaptureUrl;
use serde::Serialize;
use url::Url;

/// Version of the rules below. Version 0 means "stored as entered" (no
/// normalization), which is what a row has before its first renormalization.
/// 2: Dailymotion is recognized (it was generic).
pub const NORMALIZATION_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Youtube,
    Facebook,
    Instagram,
    Tiktok,
    X,
    Vk,
    Dailymotion,
    Generic,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Youtube => "youtube",
            Platform::Facebook => "facebook",
            Platform::Instagram => "instagram",
            Platform::Tiktok => "tiktok",
            Platform::X => "x",
            Platform::Vk => "vk",
            Platform::Dailymotion => "dailymotion",
            Platform::Generic => "generic",
        }
    }

    /// Name shown in the derived title (CAP-04).
    pub fn display_name(self) -> &'static str {
        match self {
            Platform::Youtube => "YouTube",
            Platform::Facebook => "Facebook",
            Platform::Instagram => "Instagram",
            Platform::Tiktok => "TikTok",
            Platform::X => "X",
            Platform::Vk => "VK",
            Platform::Dailymotion => "Dailymotion",
            Platform::Generic => "Web",
        }
    }
}

/// What the URL points at, as far as can be told offline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// One video with a known id.
    Video,
    /// A playlist, channel or profile: saved as one bookmark, never expanded
    /// (§6, `PLAYLIST_UNSUPPORTED` warning).
    Collection,
    /// A short link whose target is unknown until it is resolved online.
    ShortLink,
    /// Anything else.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub platform: Platform,
    pub shape: Shape,
    /// Platform content id when the URL carries one.
    pub source_id: Option<String>,
    pub canonical_url: String,
    /// `original_url` with secret parameter values replaced (CAP-08).
    pub redacted_original: String,
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizeError {
    /// The URL carries a temporary secret and no canonical form without it
    /// exists (AC-52): `URL_HAS_CREDENTIALS`, nothing is stored.
    SecretWithoutCanonical,
}

/// Query parameter names that carry signatures, tokens or temporary access
/// (CAP-08). Compared lower-case; `x-amz-*` and `x-goog-*` by prefix.
const SECRET_PARAMS: &[&str] = &[
    "access_key",
    "access_token",
    "token",
    "auth",
    "authorization",
    "sig",
    "signature",
    "sign",
    "key-pair-id",
    "policy",
    "expires",
    "session",
    "sessionid",
    "sid",
    "oauth_token",
    "api_key",
    "apikey",
    "hdnts",
    "hdnea",
    "__token__",
    "jwt",
    "password",
    "pwd",
    "secret",
];
const SECRET_PREFIXES: &[&str] = &["x-amz-", "x-goog-", "x-ms-"];

pub const REDACTED: &str = "REDACTED";

pub fn is_secret_param(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    SECRET_PARAMS.contains(&n.as_str()) || SECRET_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// The URL with every secret parameter value replaced by `REDACTED`, other
/// parameters untouched. Used for `original_url` and for anything that may
/// reach a log or diagnostics.
pub fn redact(url: &Url) -> String {
    if !url.query_pairs().any(|(k, _)| is_secret_param(&k)) {
        return url.as_str().to_owned();
    }
    let mut u = url.clone();
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| {
            let v = if is_secret_param(&k) {
                REDACTED.to_owned()
            } else {
                v.into_owned()
            };
            (k.into_owned(), v)
        })
        .collect();
    u.query_pairs_mut().clear().extend_pairs(pairs);
    u.as_str().to_owned()
}

fn host_of(url: &Url) -> String {
    url.host_str()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn host_is(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn segments(url: &Url) -> Vec<String> {
    url.path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).map(str::to_owned).collect())
        .unwrap_or_default()
}

fn query(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

fn valid(id: &str, min: usize, max: usize, extra: &[char]) -> bool {
    (min..=max).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
}

fn youtube_id(s: &str) -> Option<String> {
    (s.len() == 11 && valid(s, 11, 11, &['-', '_'])).then(|| s.to_owned())
}

fn digits(s: &str) -> Option<String> {
    (!s.is_empty() && s.len() <= 25 && s.chars().all(|c| c.is_ascii_digit())).then(|| s.to_owned())
}

struct Match {
    platform: Platform,
    shape: Shape,
    id: Option<String>,
    canonical: Option<String>,
}

fn video(platform: Platform, id: String, canonical: String) -> Option<Match> {
    Some(Match {
        platform,
        shape: Shape::Video,
        id: Some(id),
        canonical: Some(canonical),
    })
}

fn other(platform: Platform, shape: Shape) -> Option<Match> {
    Some(Match {
        platform,
        shape,
        id: None,
        canonical: None,
    })
}

fn youtube(host: &str, url: &Url) -> Option<Match> {
    let seg = segments(url);
    let canon = |id: String| {
        video(
            Platform::Youtube,
            id.clone(),
            format!("https://www.youtube.com/watch?v={id}"),
        )
    };
    if host == "youtu.be" {
        return seg
            .first()
            .and_then(|s| youtube_id(s))
            .and_then(canon)
            .or_else(|| other(Platform::Youtube, Shape::Unknown));
    }
    if !(host_is(host, "youtube.com") || host_is(host, "youtube-nocookie.com")) {
        return None;
    }
    match seg.first().map(String::as_str) {
        Some("watch") => {
            if let Some(id) = query(url, "v").and_then(|v| youtube_id(&v)) {
                return canon(id);
            }
        }
        Some("shorts" | "embed" | "live" | "v" | "e") => {
            if let Some(id) = seg.get(1).and_then(|s| youtube_id(s)) {
                return canon(id);
            }
        }
        Some("playlist" | "channel" | "c" | "user") => {
            return other(Platform::Youtube, Shape::Collection)
        }
        Some(s) if s.starts_with('@') => return other(Platform::Youtube, Shape::Collection),
        _ => {}
    }
    other(Platform::Youtube, Shape::Unknown)
}

/// `video-123_456` / `video123_456` (owner may be negative for groups).
fn vk_video(s: &str) -> Option<String> {
    let rest = s.strip_prefix("video")?;
    let (owner, id) = rest.split_once('_')?;
    let owner_digits = owner.strip_prefix('-').unwrap_or(owner);
    (digits(owner_digits).is_some() && digits(id).is_some()).then(|| format!("{owner}_{id}"))
}

fn vk(host: &str, url: &Url) -> Option<Match> {
    if !(host_is(host, "vk.com") || host_is(host, "vkvideo.ru") || host_is(host, "vk.ru")) {
        return None;
    }
    let seg = segments(url);
    // `?z=video-1_2/...` opens a video over another page.
    let from_z = query(url, "z").and_then(|z| vk_video(z.split('/').next().unwrap_or("")));
    let id = seg.first().and_then(|s| vk_video(s)).or(from_z);
    match id {
        Some(id) => video(
            Platform::Vk,
            id.clone(),
            format!("https://vk.com/video{id}"),
        ),
        None if seg
            .first()
            .is_some_and(|s| s.starts_with("videos") || s.starts_with("playlist")) =>
        {
            other(Platform::Vk, Shape::Collection)
        }
        None => other(Platform::Vk, Shape::Unknown),
    }
}

/// `x8abc12`, also in the old `x8abc12_title-slug` form.
fn dailymotion_id(s: &str) -> Option<String> {
    let id = s.split('_').next().unwrap_or("");
    valid(id, 3, 20, &[]).then(|| id.to_owned())
}

fn dailymotion(host: &str, url: &Url) -> Option<Match> {
    let seg = segments(url);
    let canon = |id: String| {
        video(
            Platform::Dailymotion,
            id.clone(),
            format!("https://www.dailymotion.com/video/{id}"),
        )
    };
    if host_is(host, "dai.ly") {
        return seg
            .first()
            .and_then(|s| dailymotion_id(s))
            .and_then(canon)
            .or_else(|| other(Platform::Dailymotion, Shape::Unknown));
    }
    if !host_is(host, "dailymotion.com") {
        return None;
    }
    let first = seg.first().map(String::as_str);
    let id = match first {
        Some("video") => seg.get(1).and_then(|s| dailymotion_id(s)),
        Some("embed") if seg.get(1).map(String::as_str) == Some("video") => {
            seg.get(2).and_then(|s| dailymotion_id(s))
        }
        // geo.dailymotion.com/player[/<id>].html?video=<id>
        Some(p) if p.starts_with("player") => query(url, "video").and_then(|v| dailymotion_id(&v)),
        _ => None,
    };
    match id {
        Some(id) => canon(id),
        None if matches!(first, Some("playlist")) => {
            other(Platform::Dailymotion, Shape::Collection)
        }
        None => other(Platform::Dailymotion, Shape::Unknown),
    }
}

fn tiktok(host: &str, url: &Url) -> Option<Match> {
    if host == "vm.tiktok.com" || host == "vt.tiktok.com" {
        return other(Platform::Tiktok, Shape::ShortLink);
    }
    if !host_is(host, "tiktok.com") {
        return None;
    }
    let seg = segments(url);
    if let (Some(user), Some("video"), Some(id)) = (
        seg.first(),
        seg.get(1).map(String::as_str),
        seg.get(2).and_then(|s| digits(s)),
    ) {
        if let Some(u) = user
            .strip_prefix('@')
            .filter(|u| valid(u, 1, 64, &['.', '_']))
        {
            return video(
                Platform::Tiktok,
                id.clone(),
                format!("https://www.tiktok.com/@{u}/video/{id}"),
            );
        }
    }
    if seg.first().is_some_and(|s| s == "t") {
        return other(Platform::Tiktok, Shape::ShortLink);
    }
    if seg.len() == 1 && seg[0].starts_with('@') {
        return other(Platform::Tiktok, Shape::Collection);
    }
    other(Platform::Tiktok, Shape::Unknown)
}

fn instagram(host: &str, url: &Url) -> Option<Match> {
    if !(host_is(host, "instagram.com") || host == "instagr.am") {
        return None;
    }
    let seg = segments(url);
    let kind = seg.first().map(String::as_str);
    let code = seg.get(1).filter(|c| valid(c, 5, 40, &['-', '_']));
    match (kind, code) {
        (Some("p"), Some(c)) => video(
            Platform::Instagram,
            c.clone(),
            format!("https://www.instagram.com/p/{c}/"),
        ),
        (Some("reel" | "reels"), Some(c)) => video(
            Platform::Instagram,
            c.clone(),
            format!("https://www.instagram.com/reel/{c}/"),
        ),
        (Some("tv"), Some(c)) => video(
            Platform::Instagram,
            c.clone(),
            format!("https://www.instagram.com/tv/{c}/"),
        ),
        (Some(_), None) if seg.len() == 1 => other(Platform::Instagram, Shape::Collection),
        _ => other(Platform::Instagram, Shape::Unknown),
    }
}

fn x(host: &str, url: &Url) -> Option<Match> {
    if !(host_is(host, "x.com") || host_is(host, "twitter.com")) {
        return None;
    }
    let seg = segments(url);
    if let (Some(_), Some("status" | "statuses"), Some(id)) = (
        seg.first(),
        seg.get(1).map(String::as_str),
        seg.get(2).and_then(|s| digits(s)),
    ) {
        return video(
            Platform::X,
            id.clone(),
            format!("https://x.com/i/status/{id}"),
        );
    }
    if seg.len() == 1 {
        return other(Platform::X, Shape::Collection);
    }
    other(Platform::X, Shape::Unknown)
}

fn facebook(host: &str, url: &Url) -> Option<Match> {
    if host == "fb.watch" {
        return other(Platform::Facebook, Shape::ShortLink);
    }
    if !(host_is(host, "facebook.com") || host == "fb.com") {
        return None;
    }
    let seg = segments(url);
    let canon = |id: String| {
        video(
            Platform::Facebook,
            id.clone(),
            format!("https://www.facebook.com/watch/?v={id}"),
        )
    };
    match seg.first().map(String::as_str) {
        Some("watch") => {
            if let Some(id) = query(url, "v").and_then(|v| digits(&v)) {
                return canon(id);
            }
        }
        Some("reel") => {
            if let Some(id) = seg.get(1).and_then(|s| digits(s)) {
                return canon(id);
            }
        }
        Some("share") => return other(Platform::Facebook, Shape::ShortLink),
        Some(_) if seg.get(1).is_some_and(|s| s == "videos") => {
            if let Some(id) = seg.get(2).and_then(|s| digits(s)) {
                return canon(id);
            }
        }
        _ => {}
    }
    other(Platform::Facebook, Shape::Unknown)
}

/// Generic form: scheme and host lower-cased and default port dropped (the
/// `url` crate does this), trailing dot removed, fragment dropped; path and
/// query kept byte for byte.
fn generic(url: &Url) -> String {
    let mut u = url.clone();
    u.set_fragment(None);
    if let Some(h) = url.host_str() {
        if h.ends_with('.') {
            let _ = u.set_host(Some(h.trim_end_matches('.')));
        }
    }
    u.as_str().to_owned()
}

pub fn normalize(input: &CaptureUrl) -> Result<Normalized, NormalizeError> {
    let url = input.as_url();
    let host = host_of(url);
    let m = youtube(&host, url)
        .or_else(|| vk(&host, url))
        .or_else(|| dailymotion(&host, url))
        .or_else(|| tiktok(&host, url))
        .or_else(|| instagram(&host, url))
        .or_else(|| x(&host, url))
        .or_else(|| facebook(&host, url))
        .unwrap_or(Match {
            platform: Platform::Generic,
            shape: Shape::Unknown,
            id: None,
            canonical: None,
        });
    let has_secret = url.query_pairs().any(|(k, _)| is_secret_param(&k));
    let canonical_url = match m.canonical {
        Some(c) => c,
        // No rebuilt form: a secret in the URL would be stored as is.
        None if has_secret => return Err(NormalizeError::SecretWithoutCanonical),
        None => generic(url),
    };
    Ok(Normalized {
        platform: m.platform,
        shape: m.shape,
        source_id: m.id,
        canonical_url,
        redacted_original: redact(url),
        version: NORMALIZATION_VERSION,
    })
}

/// Honest title derived from the link alone (CAP-04), e.g. `YouTube · abc123`
/// or `example.com · clip-name`. Replaced by real metadata in M2.
pub fn derived_title(n: &Normalized) -> String {
    let url = Url::parse(&n.canonical_url).ok();
    match (&n.source_id, n.platform) {
        (Some(id), p) => format!("{} · {id}", p.display_name()),
        (None, Platform::Generic) => {
            let host = url.as_ref().map(host_of).unwrap_or_default();
            let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
            let last = url
                .as_ref()
                .and_then(|u| {
                    u.path_segments()
                        .and_then(|mut s| s.rfind(|p| !p.is_empty()).map(str::to_owned))
                })
                .map(|s| percent_decode(&s));
            match last {
                Some(l) if !l.is_empty() => {
                    format!("{host} · {}", l.chars().take(80).collect::<String>())
                }
                _ => host,
            }
        }
        (None, p) => format!(
            "{} · {}",
            p.display_name(),
            match n.shape {
                Shape::Collection => "liste",
                Shape::ShortLink => "kısa bağlantı",
                _ => "bağlantı",
            }
        ),
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_policy::validate_capture_url;

    fn n(s: &str) -> Normalized {
        normalize(&validate_capture_url(s).unwrap()).unwrap()
    }

    #[test]
    fn derived_titles_are_honest() {
        assert_eq!(
            derived_title(&n("https://youtu.be/vvTestVid01")),
            "YouTube · vvTestVid01"
        );
        assert_eq!(
            derived_title(&n("https://www.example.com/a/My%20Clip")),
            "example.com · My Clip"
        );
        assert_eq!(derived_title(&n("https://example.com/")), "example.com");
        assert_eq!(
            derived_title(&n("https://www.youtube.com/playlist?list=PL1")),
            "YouTube · liste"
        );
        assert_eq!(
            derived_title(&n("https://vm.tiktok.com/ZM1/")),
            "TikTok · kısa bağlantı"
        );
    }

    #[test]
    fn percent_decode_handles_edges() {
        assert_eq!(percent_decode(""), "");
        assert_eq!(percent_decode("%"), "%");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("%41%zz"), "A%zz");
        assert_eq!(percent_decode("%C3%BC"), "ü");
    }

    #[test]
    fn secret_param_names() {
        for k in [
            "Signature",
            "X-Amz-Date",
            "access_key",
            "TOKEN",
            "x-goog-signature",
        ] {
            assert!(is_secret_param(k), "{k}");
        }
        for k in ["v", "list", "si", "utm_source", "t"] {
            assert!(!is_secret_param(k), "{k}");
        }
    }
}
