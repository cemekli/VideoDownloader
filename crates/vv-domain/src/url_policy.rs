//! Capture URL policy (SPEC §7.1 CAP-02, §20.2).
//!
//! This is the *syntactic* gate that runs before any network access and works
//! offline. It rejects non-HTTP(S) schemes, URLs carrying credentials and
//! literal hosts that point at loopback, private, link-local or otherwise
//! non-public addresses. Hostnames that only *resolve* to such addresses are
//! handled by the network layer (DNS + per-connection checks, DEC-05); this
//! module cannot see DNS and does not claim to.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::{Host, Url};

/// A URL that passed the capture policy. Holds the parsed form only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureUrl {
    url: Url,
}

impl CaptureUrl {
    pub fn as_url(&self) -> &Url {
        &self.url
    }
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }
}

/// Rejection reasons. `code()` returns the stable error code from SPEC §19.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlPolicyError {
    /// Not parseable, empty, or a scheme other than http/https.
    InvalidUrl,
    /// URL contains a username or password (userinfo).
    HasCredentials,
    /// Literal host is loopback/private/link-local/etc. or `localhost`.
    Blocked,
}

impl UrlPolicyError {
    pub fn code(&self) -> &'static str {
        match self {
            UrlPolicyError::InvalidUrl => "INVALID_URL",
            UrlPolicyError::HasCredentials => "URL_HAS_CREDENTIALS",
            UrlPolicyError::Blocked => "URL_BLOCKED",
        }
    }
}

/// Validate user input for capture. Leading/trailing whitespace is ignored.
pub fn validate_capture_url(input: &str) -> Result<CaptureUrl, UrlPolicyError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(UrlPolicyError::InvalidUrl);
    }
    let url = Url::parse(trimmed).map_err(|_| UrlPolicyError::InvalidUrl)?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(UrlPolicyError::InvalidUrl);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(UrlPolicyError::HasCredentials);
    }
    match url.host() {
        None => return Err(UrlPolicyError::InvalidUrl),
        Some(Host::Domain(d)) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            if d.is_empty() {
                return Err(UrlPolicyError::InvalidUrl);
            }
            if d == "localhost" || d.ends_with(".localhost") {
                return Err(UrlPolicyError::Blocked);
            }
        }
        Some(Host::Ipv4(ip)) => {
            if !is_public_ipv4(ip) {
                return Err(UrlPolicyError::Blocked);
            }
        }
        Some(Host::Ipv6(ip)) => {
            if !is_public_ip(IpAddr::V6(ip)) {
                return Err(UrlPolicyError::Blocked);
            }
        }
    }
    Ok(CaptureUrl { url })
}

/// True only for addresses that may be contacted for remote content capture.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_ipv4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = ipv4_embedded(v6) {
                return is_public_ipv4(v4);
            }
            is_public_ipv6(v6)
        }
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local() // includes 169.254.169.254 cloud metadata
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0 // 0.0.0.0/8
        || (o[0] == 100 && (o[1] & 0xC0) == 64) // 100.64.0.0/10 shared address space
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24 IETF protocol assignments
        || (o[0] == 198 && (o[1] & 0xFE) == 18) // 198.18.0.0/15 benchmarking
        || o[0] >= 240) // 240.0.0.0/4 reserved
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let seg = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (seg[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local
        || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 link local
        || (seg[0] & 0xffc0) == 0xfec0 // fec0::/10 site local (deprecated)
        || (seg[0] == 0x2001 && seg[1] == 0x0db8)) // 2001:db8::/32 documentation
}

/// IPv4-mapped (::ffff:a.b.c.d), IPv4-compatible (::a.b.c.d) and NAT64 (64:ff9b::/96).
fn ipv4_embedded(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let s = ip.segments();
    let tail = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
    if s[0..5] == [0, 0, 0, 0, 0] && (s[5] == 0xffff || (s[5] == 0 && !(s[6] == 0 && s[7] <= 1))) {
        return Some(tail);
    }
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
        return Some(tail);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_public_https() {
        assert!(validate_capture_url("https://www.youtube.com/watch?v=abc").is_ok());
        assert!(validate_capture_url("  http://vk.com/video-1_2  ").is_ok());
    }

    #[test]
    fn rejects_non_http_schemes() {
        for u in [
            "file:///C:/x.mp4",
            "javascript:alert(1)",
            "data:text/html,hi",
            "ftp://a.com/x",
            "ms-settings:",
        ] {
            assert_eq!(
                validate_capture_url(u),
                Err(UrlPolicyError::InvalidUrl),
                "{u}"
            );
        }
    }

    #[test]
    fn rejects_credentials() {
        assert_eq!(
            validate_capture_url("https://user:pw@example.com/v")
                .unwrap_err()
                .code(),
            "URL_HAS_CREDENTIALS"
        );
        assert_eq!(
            validate_capture_url("https://user@example.com/v"),
            Err(UrlPolicyError::HasCredentials)
        );
    }

    #[test]
    fn ipv4_mapped_ipv6_follows_ipv4_rules() {
        assert!(!is_public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("::ffff:10.0.0.1".parse().unwrap()));
        assert!(is_public_ip("::ffff:93.184.216.34".parse().unwrap()));
        assert!(!is_public_ip("64:ff9b::a9fe:a9fe".parse().unwrap())); // NAT64 169.254.169.254
    }
}
