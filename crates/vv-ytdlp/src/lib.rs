//! VideoVault yt-dlp adapter (SPEC §14.1, DL-05; STATE.md F0.6).
//!
//! This crate never starts a process. It owns three things:
//!
//! - [`args`]: the fixed flags every yt-dlp run gets, including the progress
//!   template that turns progress into one machine-readable line per update.
//! - Parsers for what yt-dlp writes: [`metadata`] (`--dump-single-json`),
//!   [`progress`] (our template lines) and [`outcome`] (exit status + stderr →
//!   SPEC §19 error code).
//! - Output hygiene: [`lines`] splits a byte stream into bounded lines and
//!   [`redact`] strips URLs' secrets, cookies and user names before anything
//!   is kept.
//!
//! The process side (spawning, Job Objects, cancel) is F0.10; the tests here
//! run on captured output in `tests/fixtures/ytdlp/`.

pub mod args;
pub mod captions;
pub mod lines;
pub mod metadata;
pub mod outcome;
pub mod progress;
pub mod redact;

mod pyjson;
