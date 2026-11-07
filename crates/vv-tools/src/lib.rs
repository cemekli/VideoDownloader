//! Managed external tools (SPEC §10.2, §14.1, §23.1; STATE.md F0.8b, ADR-0005).
//!
//! `tools/manifest.json` pins each tool's version, download URL, SHA-256,
//! size and licence. This crate reads and validates that manifest, verifies
//! files against it and checks what a tool reports about its own version.
//! It never downloads anything and never starts a process: the developer
//! script `scripts/fetch-tools.mjs` downloads, F0.10 runs processes.

pub mod health;
pub mod manifest;
pub mod verify;

pub use health::{
    check_version, parse_deno_version, parse_js_runtimes_line, parse_ytdlp_version, JsRuntimeLine,
    Version, VersionCheck, DENO_MIN_FOR_YTDLP,
};
pub use manifest::{Artifact, Manifest, ManifestError, Provenance, Tool, ToolId};
pub use verify::{verify_file, VerifyError};
