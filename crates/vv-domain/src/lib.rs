//! VideoVault domain crate.
//!
//! Pure rules only: no filesystem, network, subprocess or platform paths
//! (SPEC §10.3). Everything here must be testable on any OS without setup.

pub mod url_policy;
