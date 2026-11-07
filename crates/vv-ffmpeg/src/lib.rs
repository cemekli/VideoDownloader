//! FFmpeg and ffprobe adapter (SPEC §14.1, IMP-02, AC-59; STATE.md F0.8c,
//! ADR-0006).
//!
//! Fixed arguments that keep both tools on local files only, and the
//! classification of what the restricted ffprobe reports. Like `vv-ytdlp`,
//! this crate never starts a process; F0.10 does.

pub mod args;
pub mod probe;

pub use probe::{
    classify, parse_probe_json, AudioTrack, Container, ProbeError, ProbeReport, Probed,
};
