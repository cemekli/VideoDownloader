//! Fixed yt-dlp flags (SPEC §14.1).
//!
//! Mirrors `tests/fixtures/ytdlp/args.json`, which the capture script uses; a
//! test fails if the two drift. Run-specific parts (URL, output directory and
//! template, format selector) are appended by the caller as separate argv
//! entries, never through a shell.

/// yt-dlp release the fixtures were captured with. A new release means
/// re-running `scripts/capture-ytdlp-fixtures.mjs` and the tests.
pub const PINNED_VERSION: &str = "2026.10.26";

/// Prefix of a download progress line written by [`DOWNLOAD`]'s template.
pub const PROGRESS_PREFIX: &str = "vv-progress ";
/// Prefix of a post-processor progress line.
pub const POSTPROCESS_PREFIX: &str = "vv-postprocess ";

/// Every run: no user/system config files, no plugin directories, no cache,
/// no colour codes in the output.
pub const COMMON: &[&str] = &[
    "--ignore-config",
    "--no-plugin-dirs",
    "--no-cache-dir",
    "--color",
    "never",
];

/// Metadata only. `--no-playlist --flat-playlist` keeps a playlist URL from
/// resolving every entry; the result is reported as `PLAYLIST_UNSUPPORTED`.
pub const METADATA: &[&str] = &["--dump-single-json", "--no-playlist", "--flat-playlist"];

/// Download. `--quiet --progress --newline` leaves only template lines and
/// errors. Missing template fields print as `NA`, which [`crate::progress`]
/// reads as null.
pub const DOWNLOAD: &[&str] = &[
    "--quiet",
    "--progress",
    "--newline",
    "--no-playlist",
    "--progress-template",
    concat!(
        "download:vv-progress {",
        "\"status\":%(progress.status)j,",
        "\"downloaded_bytes\":%(progress.downloaded_bytes)j,",
        "\"total_bytes\":%(progress.total_bytes)j,",
        "\"total_bytes_estimate\":%(progress.total_bytes_estimate)j,",
        "\"eta\":%(progress.eta)j,",
        "\"speed\":%(progress.speed)j,",
        "\"elapsed\":%(progress.elapsed)j,",
        "\"fragment_index\":%(progress.fragment_index)j,",
        "\"fragment_count\":%(progress.fragment_count)j}"
    ),
    "--progress-template",
    "postprocess:vv-postprocess {\"status\":%(progress.status)j,\"postprocessor\":%(progress.postprocessor)j}",
];

/// JavaScript runtime (STATE.md F0.8b, ADR-0005). yt-dlp's own default enables
/// Deno from `PATH`; `--no-js-runtimes` clears that so only the managed Deno at
/// an explicit path is used (SPEC §14.1). `--no-remote-components` keeps
/// yt-dlp from fetching the EJS solver scripts at run time: the official
/// executables already bundle the pinned `yt-dlp-ejs`.
pub const JS_RUNTIME: &[&str] = &[
    "--no-js-runtimes",
    "--js-runtimes",
    "deno:{deno}",
    "--no-remote-components",
];

/// [`JS_RUNTIME`] with the managed Deno executable filled in, as separate argv
/// entries. Without a path no runtime is enabled at all.
pub fn js_runtime_args(deno: Option<&std::path::Path>) -> Vec<std::ffi::OsString> {
    let Some(deno) = deno else {
        return vec!["--no-js-runtimes".into(), "--no-remote-components".into()];
    };
    JS_RUNTIME
        .iter()
        .map(|a| match *a {
            "deno:{deno}" => {
                let mut v = std::ffi::OsString::from("deno:");
                v.push(deno.as_os_str());
                v
            }
            other => other.into(),
        })
        .collect()
}

/// Network guard (STATE.md F0.9, ADR-0007). Every request goes through the
/// app's local proxy, which checks each connection (`vv-netguard`).
/// `--proxy` overrides `HTTP(S)_PROXY`, `ALL_PROXY` and `NO_PROXY` from the
/// environment (verified with the pinned yt-dlp). `--downloader native`
/// keeps yt-dlp from handing a protocol to FFmpeg, which would open its own
/// connections; protocols the native downloader cannot handle fail instead.
/// `--abort-on-unavailable-fragments` makes a blocked HLS/DASH segment fail
/// the download rather than leave a gap.
pub const NETWORK_GUARD: &[&str] = &[
    "--proxy",
    "{proxy}",
    "--downloader",
    "native",
    "--abort-on-unavailable-fragments",
];

/// [`NETWORK_GUARD`] with the guard's proxy URL. The URL carries the
/// per-run token: it is passed as one argv entry and never logged.
pub fn network_guard_args(proxy_url: &str) -> Vec<std::ffi::OsString> {
    NETWORK_GUARD
        .iter()
        .map(|a| {
            if *a == "{proxy}" {
                proxy_url.into()
            } else {
                (*a).into()
            }
        })
        .collect()
}
