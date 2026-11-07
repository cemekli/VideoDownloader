//! Network guard for managed tools (SPEC §20.2, AC-29; STATE.md F0.9,
//! ADR-0007).
//!
//! Checking a URL once and then handing it to yt-dlp does not stop SSRF:
//! yt-dlp follows redirects, reads manifests and fetches segments on its own.
//! So every connection yt-dlp makes goes through a local proxy in the app
//! process (`--proxy`), and the proxy decides per connection:
//!
//! - the target port must be 80 or 443;
//! - a host name is resolved once, *every* address must be public
//!   ([`vv_domain::url_policy::is_public_ip`]), and the proxy connects to the
//!   address it checked, so a second DNS answer cannot redirect it
//!   (DNS rebinding);
//! - only clients that know the per-run token may use the proxy.
//!
//! The proxy records host, port and decision, never paths or queries.

pub mod policy;
pub mod proxy;

pub use policy::{Block, Policy, Resolver, SystemResolver};
pub use proxy::{Decision, Event, Guard};
