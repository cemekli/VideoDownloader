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
