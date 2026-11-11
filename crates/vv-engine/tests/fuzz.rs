//! Fuzz tests for the engine's own parsers of files the user gives us
//! (docs/testing.md "Fuzz testleri"): Netscape cookie files (F5.3) and
//! Firefox's `profiles.ini` (F5.2). Never a panic; only the platform's
//! cookies are kept; a relative profile never leaves Firefox's folder.

use std::path::Component;
use vv_domain::normalize::Platform;
use vv_engine::browsers::firefox_profiles;
use vv_engine::cookiefile::{domains, filter};
use vv_fuzz::{each, mutate_bytes, mutate_str, prefixes, run};

const COOKIES: &str = "# Netscape HTTP Cookie File\n\
# kullanıcı: Ayşe Çağlar 😀\n\
.reddit.com\tTRUE\t/\tTRUE\t0\treddit_session\tVVCANARY-ş\n\
#HttpOnly_.www.reddit.com\tTRUE\t/\tTRUE\t4102444800\tloid\tabc\n\
.reddit.com.evil.example\tTRUE\t/\tTRUE\t0\tx\ty\n\
evilreddit.com\tTRUE\t/\tTRUE\t0\tx\ty\n\
.youtube.com\tTRUE\t/\tTRUE\t0\tSID\tz\r\n\
.vk.com\tTRUE\t/\tFALSE\t1\told\texpired\n";

const COOKIE_PIECES: &[&str] = &[
    "\t",
    "\n",
    "\r\n",
    "TRUE",
    "FALSE",
    "/",
    ".",
    ".reddit.com",
    "reddit.com",
    ".com",
    "#HttpOnly_",
    "#",
    "0",
    "-1",
    "99999999999999999999",
    "ŞİRKET.reddit.com",
    ".REDDİT.com",
    "xn--",
];

fn platform(i: usize) -> Platform {
    [
        Platform::Reddit,
        Platform::Youtube,
        Platform::Vk,
        Platform::Instagram,
        Platform::Generic,
    ][i % 5]
}

fn check_cookies((text, p): &(String, Platform)) {
    let Ok(f) = filter(text, *p, 1_800_000_000) else {
        return;
    };
    assert!(f.count > 0);
    let bases = domains(*p);
    for line in f.text.lines().skip(1) {
        let body = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        let domain = body
            .split('\t')
            .next()
            .unwrap()
            .trim_start_matches('.')
            .to_ascii_lowercase();
        assert!(
            bases
                .iter()
                .any(|b| domain == *b || domain.ends_with(&format!(".{b}"))),
            "{domain} kept for {p:?}"
        );
    }
}

#[test]
fn cookie_files() {
    each(
        "cookies/prefixes",
        prefixes(COOKIES)
            .into_iter()
            .map(|s| (s.to_owned(), Platform::Reddit)),
        check_cookies,
    );
    run(
        "cookies",
        |r| {
            let text = if r.chance(50) {
                mutate_str(r, COOKIES, COOKIE_PIECES)
            } else {
                String::from_utf8_lossy(&mutate_bytes(r, COOKIES.as_bytes(), COOKIE_PIECES))
                    .into_owned()
            };
            (text, platform(r.below(5)))
        },
        check_cookies,
    );
}

const PROFILES: &str =
    "[Install308046B0AF4A39CB]\nDefault=Profiles/abc.default-release\nLocked=1\n\n\
[Profile0]\nName=Ayşe'nin profili 😀\nIsRelative=1\nPath=Profiles/abc.default-release\n\n\
[Profile1]\nName=eski\nIsRelative=1\nPath=Profiles/x.default\nDefault=1\n\n\
[General]\nStartWithLastProfile=1\nVersion=2\n";

#[test]
fn firefox_profiles_ini() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let ini = root.join("profiles.ini");
    const PIECES: &[&str] = &[
        "[",
        "]",
        "=",
        "\n",
        "[Profile9]\n",
        "Path=",
        "IsRelative=0\n",
        "IsRelative=1\n",
        "Default=1\n",
        "..",
        "../",
        "..\\",
        "C:\\",
        "C:",
        "/etc",
        "\\\\server\\share",
        "Profiles/",
        "Name=",
    ];
    let check = |bytes: &Vec<u8>| {
        std::fs::write(&ini, bytes).unwrap();
        if String::from_utf8_lossy(bytes).contains("IsRelative=0") {
            let _ = firefox_profiles(root);
            return;
        }
        // Every profile came from a relative path: it stays under the root.
        for p in firefox_profiles(root) {
            let rest = p
                .path
                .strip_prefix(root)
                .unwrap_or_else(|_| panic!("{:?} is outside {root:?}", p.path));
            assert!(
                rest.components().all(|c| matches!(c, Component::Normal(_))),
                "{:?} leaves the folder",
                p.path
            );
        }
    };
    for s in prefixes(PROFILES) {
        check(&s.as_bytes().to_vec());
    }
    run(
        "profiles.ini",
        |r| {
            if r.chance(50) {
                mutate_str(r, PROFILES, PIECES).into_bytes()
            } else {
                mutate_bytes(r, PROFILES.as_bytes(), PIECES)
            }
        },
        |b| check(b),
    );
}
