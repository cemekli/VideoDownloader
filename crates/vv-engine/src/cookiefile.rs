//! A cookie file the user exported, narrowed to one platform (STATE.md
//! F5.3; SPEC §20.3, §21.1, VK-10; ADR-0024).
//!
//! The file is read once, when the user connects it. Only the cookies of
//! the chosen platform's own domains are kept; everything else in the file
//! (other sites, comments, malformed lines, cookies already expired) is
//! dropped. The kept part goes to the OS secret store; the user's file is
//! never written and never kept.
//!
//! Format: Netscape `cookies.txt`, seven tab-separated fields per line:
//! domain, include-subdomains flag, path, secure flag, expiry (Unix
//! seconds, `0` for a session cookie), name, value. A `#HttpOnly_` prefix
//! on the domain marks an HttpOnly cookie; other `#` lines are comments.

use vv_domain::normalize::Platform;

/// Largest file read (a whole browser's export is a few MiB at most).
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

const HEADER: &str = "# Netscape HTTP Cookie File\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieFileError {
    /// Not readable, too large, or not text.
    Unreadable,
    /// No usable cookie of this platform in the file.
    NoCookiesForSite,
    /// The platform's part is larger than the secret store takes.
    TooLarge,
}

impl CookieFileError {
    /// Stable reason for the UI (`session-file-*`).
    pub fn reason(self) -> &'static str {
        match self {
            CookieFileError::Unreadable => "session-file-unreadable",
            CookieFileError::NoCookiesForSite => "session-file-no-cookies",
            CookieFileError::TooLarge => "session-file-too-large",
        }
    }
}

/// The platform's own cookie domains. A cookie is kept when its domain is
/// one of these or a subdomain of one.
pub fn domains(p: Platform) -> &'static [&'static str] {
    match p {
        Platform::Youtube => &["youtube.com"],
        Platform::Facebook => &["facebook.com"],
        Platform::Instagram => &["instagram.com"],
        Platform::Tiktok => &["tiktok.com"],
        Platform::X => &["x.com", "twitter.com"],
        Platform::Vk => &["vk.com", "vk.ru", "vkvideo.ru"],
        Platform::Dailymotion => &["dailymotion.com"],
        Platform::Twitch => &["twitch.tv"],
        Platform::Reddit => &["reddit.com", "redd.it"],
        _ => &[],
    }
}

fn domain_matches(domain: &str, bases: &[&str]) -> bool {
    let d = domain.trim_start_matches('.').to_ascii_lowercase();
    !d.is_empty()
        && bases.iter().any(|b| {
            d == *b
                || (d.len() > b.len() && d.ends_with(b) && d[..d.len() - b.len()].ends_with('.'))
        })
}

/// The platform's cookies from a Netscape file, as a Netscape file.
#[derive(PartialEq, Eq)]
pub struct Filtered {
    pub text: String,
    pub count: usize,
}

impl std::fmt::Debug for Filtered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Filtered {{ count: {}, text: [redacted] }}", self.count)
    }
}

/// Keeps the lines of `text` that are well-formed, unexpired cookies of
/// `platform`. `now_secs` is the current Unix time.
pub fn filter(text: &str, platform: Platform, now_secs: i64) -> Result<Filtered, CookieFileError> {
    let bases = domains(platform);
    if bases.is_empty() {
        return Err(CookieFileError::NoCookiesForSite);
    }
    let mut out = String::from(HEADER);
    let mut count = 0;
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let body = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        if body.is_empty() || body.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = body.split('\t').collect();
        if f.len() != 7 {
            continue;
        }
        let flags_ok = matches!(f[1], "TRUE" | "FALSE") && matches!(f[3], "TRUE" | "FALSE");
        let Ok(expiry) = f[4].parse::<i64>() else {
            continue;
        };
        if !flags_ok || f[5].is_empty() || !f[2].starts_with('/') {
            continue;
        }
        if expiry != 0 && expiry < now_secs {
            continue;
        }
        if !domain_matches(f[0], bases) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        count += 1;
    }
    if count == 0 {
        return Err(CookieFileError::NoCookiesForSite);
    }
    if out.len() > vv_secrets::MAX_SECRET_BYTES {
        return Err(CookieFileError::TooLarge);
    }
    Ok(Filtered { text: out, count })
}

/// Reads the user's file (never writes it) and filters it.
pub fn read_and_filter(
    path: &std::path::Path,
    platform: Platform,
    now_secs: i64,
) -> Result<Filtered, CookieFileError> {
    use std::io::Read;
    let f = std::fs::File::open(path).map_err(|_| CookieFileError::Unreadable)?;
    let len = f.metadata().map_err(|_| CookieFileError::Unreadable)?.len();
    if len > MAX_FILE_BYTES {
        return Err(CookieFileError::Unreadable);
    }
    let mut bytes = Vec::with_capacity(len as usize);
    f.take(MAX_FILE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|_| CookieFileError::Unreadable)?;
    let text = String::from_utf8(bytes).map_err(|_| CookieFileError::Unreadable)?;
    filter(&text, platform, now_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# Netscape HTTP Cookie File\n\
# This is a generated file!\n\
.reddit.com\tTRUE\t/\tTRUE\t0\treddit_session\tR-SECRET\n\
#HttpOnly_.www.reddit.com\tTRUE\t/\tTRUE\t4102444800\ttoken_v2\tR-TOKEN\n\
old.reddit.com\tFALSE\t/\tFALSE\t1000\texpired\tR-OLD\n\
.notreddit.com\tTRUE\t/\tFALSE\t0\tlookalike\tEVIL\n\
.reddit.com.evil.example\tTRUE\t/\tFALSE\t0\tsuffix\tEVIL2\n\
.google.com\tTRUE\t/\tTRUE\t0\tSID\tGOOGLE\n\
.reddit.com\tTRUE\t/\tTRUE\tnot-a-number\tbad\tBAD\n\
.reddit.com\tTRUE\t/\n\
\n";

    #[test]
    fn keeps_only_the_platforms_live_cookies() {
        let f = filter(FILE, Platform::Reddit, 2_000_000_000).unwrap();
        assert_eq!(f.count, 2);
        assert!(f.text.starts_with("# Netscape HTTP Cookie File\n"));
        assert!(f.text.contains("R-SECRET"));
        assert!(f.text.contains("#HttpOnly_.www.reddit.com"));
        for gone in ["R-OLD", "EVIL", "EVIL2", "GOOGLE", "BAD", "generated"] {
            assert!(!f.text.contains(gone), "{gone} must be dropped");
        }
        assert!(!format!("{f:?}").contains("R-SECRET"));
    }

    #[test]
    fn a_file_without_the_sites_cookies_is_refused() {
        assert_eq!(
            filter(FILE, Platform::Vk, 2_000_000_000),
            Err(CookieFileError::NoCookiesForSite)
        );
        assert_eq!(
            filter(FILE, Platform::Generic, 0),
            Err(CookieFileError::NoCookiesForSite)
        );
        assert_eq!(
            filter("not a cookie file", Platform::Reddit, 0),
            Err(CookieFileError::NoCookiesForSite)
        );
    }

    #[test]
    fn domains_match_whole_labels_only() {
        assert!(domain_matches(".vk.com", domains(Platform::Vk)));
        assert!(domain_matches("m.vk.com", domains(Platform::Vk)));
        assert!(!domain_matches("notvk.com", domains(Platform::Vk)));
        assert!(!domain_matches(".", domains(Platform::Vk)));
    }

    #[test]
    fn reading_never_changes_the_users_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("cookies.txt");
        std::fs::write(&p, FILE).unwrap();
        let before = std::fs::metadata(&p).unwrap().modified().unwrap();
        let f = read_and_filter(&p, Platform::Reddit, 2_000_000_000).unwrap();
        assert_eq!(f.count, 2);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), FILE);
        assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), before);
        assert_eq!(
            read_and_filter(&d.path().join("missing"), Platform::Reddit, 0),
            Err(CookieFileError::Unreadable)
        );
    }
}
