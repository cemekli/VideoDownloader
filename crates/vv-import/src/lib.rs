//! Local media on disk (SPEC §7.2 IMP-01…07, §7.7 DATA-09/DATA-10, §14.1,
//! §14.2; STATE.md F1.4/F1.5; ADR-0012).
//!
//! - [`import`]: files and folders become `local_video` items that reference
//!   the file where it is (`external_reference`); nothing is copied, moved or
//!   written next to the source.
//! - [`roots`]: whether each storage root is reachable, and the per-asset
//!   `missing` state, which is only decided while the root is reachable.
//! - [`relink`]: points an item at the moved file again.
//! - [`thumbnail`]: one frame per video into the cache root, after import.
//!
//! All ffprobe/FFmpeg runs go through `vv-process` (clean environment, job
//! object, timeout) with the fixed `vv-ffmpeg` arguments: local files only.

pub mod fsutil;
pub mod import;
pub mod relink;
pub mod roots;
pub mod thumbnail;
pub mod tools;

pub use tools::MediaTools;
