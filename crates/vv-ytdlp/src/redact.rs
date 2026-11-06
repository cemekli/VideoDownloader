//! Redaction of yt-dlp diagnostics before they are kept anywhere (SPEC §14.1:
//! "stderr için sınırlı ve redakte hata işleme"; §20.1 cookie/profile row).
//!
//! What goes:
//! - URL user info, path, query and fragment: only `scheme://host[:port]`
//!   stays, so a signed media URL or a private video path cannot leak.
//! - Everything after a `Cookie:`, `Set-Cookie:` or `Authorization:` header.
//! - The user name in `C:\Users\<name>`, `/home/<name>` and `/Users/<name>`.
//!
//! The result is also cut to [`MAX_REDACTED_CHARS`].

pub const MAX_REDACTED_CHARS: usize = 500;

const HEADER_MARKERS: [&str; 4] = [
    "proxy-authorization:",
    "authorization:",
    "set-cookie:",
    "cookie:",
];
const HOME_MARKERS: [&str; 3] = ["/home/", "/Users/", "\\Users\\"];

pub fn redact(line: &str) -> String {
    let mut s = cut_headers(line);
    s = redact_urls(&s);
    s = redact_home_dirs(&s);
    if s.chars().count() > MAX_REDACTED_CHARS {
        s = s.chars().take(MAX_REDACTED_CHARS).collect::<String>() + "…";
    }
    s
}

fn cut_headers(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    match HEADER_MARKERS.iter().filter_map(|m| lower.find(m)).min() {
        Some(i) => format!("{}<redacted>", &line[..i]),
        None => line.to_string(),
    }
}

fn redact_urls(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = find_scheme(rest) {
        out.push_str(&rest[..start]);
        let url = &rest[start..];
        let end = url
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | '(' | ',')
            })
            .unwrap_or(url.len());
        out.push_str(&keep_origin(&url[..end]));
        rest = &url[end..];
    }
    out.push_str(rest);
    out
}

fn find_scheme(s: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    ["https://", "http://", "ftp://", "wss://", "ws://"]
        .iter()
        .filter_map(|p| lower.find(p))
        .min()
}

/// `https://u:p@host:1/a?b#c` → `https://host:1/…`; `https://host/` stays.
fn keep_origin(url: &str) -> String {
    let (scheme, after) = url.split_once("://").unwrap_or(("", url));
    let auth_end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let authority = &after[..auth_end];
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let tail = &after[auth_end..];
    let suffix = if tail.is_empty() || tail == "/" {
        tail
    } else {
        "/…"
    };
    format!("{scheme}://{host}{suffix}")
}

fn redact_home_dirs(line: &str) -> String {
    let mut s = line.to_string();
    for marker in HOME_MARKERS {
        s = replace_segment_after(&s, marker);
    }
    // Python reprs double the backslashes: C:\\Users\\name\\...
    replace_segment_after(&s, "\\\\Users\\\\")
}

fn replace_segment_after(s: &str, marker: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(marker) {
        let after = &rest[i + marker.len()..];
        let name_end = after
            .find(['/', '\\', '\'', '"', ' '])
            .unwrap_or(after.len());
        out.push_str(&rest[..i + marker.len()]);
        out.push_str(if name_end == 0 { "" } else { "<user>" });
        rest = &after[name_end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn urls_keep_only_their_origin() {
        assert_eq!(
            redact("x https://u:p@cdn.example.test:8443/a/b.mp4?sig=1#t and http://h.test/"),
            "x https://cdn.example.test:8443/… and http://h.test/"
        );
        assert_eq!(
            redact("(caused by <HTTPError 403: Forbidden>)"),
            "(caused by <HTTPError 403: Forbidden>)"
        );
    }

    #[test]
    fn headers_and_home_dirs() {
        assert_eq!(redact("got Cookie: a=b; c=d"), "got <redacted>");
        assert_eq!(redact("AUTHORIZATION: Bearer x"), "<redacted>");
        assert_eq!(
            redact(r"denied: 'C:\Users\jdoe\Videos\a.part' /home/jdoe/x /Users/jo"),
            r"denied: 'C:\Users\<user>\Videos\a.part' /home/<user>/x /Users/<user>"
        );
        assert_eq!(redact(r"'C:\\Users\\jdoe\\V'"), r"'C:\\Users\\<user>\\V'");
    }

    #[test]
    fn output_is_bounded() {
        let long = "é".repeat(5_000);
        assert_eq!(redact(&long).chars().count(), super::MAX_REDACTED_CHARS + 1);
    }
}
