//! VideoVault download engine (STATE.md F3; SPEC §6, §7.5, §13, §14, §20.2;
//! ADR-0021).
//!
//! - [`toolset`]: managed tool health check at start-up (F3.2).
//! - [`ytdlp`]: one yt-dlp run behind its own network guard (F3.3).
//! - [`metadata`]: remote metadata and thumbnail jobs (F3.4).
//! - [`formats`]: quality presets from real format data (F3.5).
//! - [`download`]: download, merge, verification, staging → final (F3.6).
//! - [`storage`], [`storage_move`]: quota, usage and moving a root (F3.9).
//! - [`runtime`]: the scheduler and its worker pools.
//!
//! Process handling is `vv-process`, queue state is
//! `vv-persistence::jobs`, error handling `vv-persistence::classify`.

pub mod context;
pub mod download;
pub mod formats;
pub mod metadata;
pub mod proc;
pub mod runtime;
pub mod storage;
pub mod storage_move;
pub mod toolset;
pub mod worker;
pub mod ytdlp;

pub use context::{Context, Dirs, Events, NoEvents, Store, Tools};
pub use runtime::Engine;
