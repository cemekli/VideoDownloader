//! Per-connection decision: may the proxy connect to `host:port`?

use serde::Serialize;
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use vv_domain::url_policy::is_public_ip;

/// Why a connection was refused. Every variant is reported as `URL_BLOCKED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Block {
    /// Not 80 or 443.
    Port { port: u16 },
    /// `localhost`, `*.localhost` or an empty name.
    LocalName,
    /// A literal or resolved address that is not public.
    NonPublic { ip: IpAddr },
    /// DNS failed or returned nothing.
    Unresolved,
    /// The request itself was malformed or used an unsupported form.
    BadRequest,
}

pub trait Resolver: Send + Sync {
    fn resolve(&self, host: &str, port: u16) -> io::Result<Vec<IpAddr>>;
}

/// The operating system's resolver.
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve(&self, host: &str, port: u16) -> io::Result<Vec<IpAddr>> {
        Ok((host, port).to_socket_addrs()?.map(|a| a.ip()).collect())
    }
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub allowed_ports: Vec<u16>,
    /// Test only: exact loopback targets the local test servers listen on.
    /// The shipped app never sets this (ADR-0007).
    pub test_targets: Vec<SocketAddr>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            allowed_ports: vec![80, 443],
            test_targets: Vec::new(),
        }
    }
}

impl Policy {
    /// Returns the one address the proxy will connect to.
    pub fn vet(&self, resolver: &dyn Resolver, host: &str, port: u16) -> Result<SocketAddr, Block> {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        let host = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(&host);
        let literal: Option<IpAddr> = host.parse().ok();
        if let Some(ip) = literal {
            let addr = SocketAddr::new(ip, port);
            if self.test_targets.contains(&addr) {
                return Ok(addr);
            }
        }
        if !self.allowed_ports.contains(&port) {
            return Err(Block::Port { port });
        }
        let ips = match literal {
            Some(ip) => vec![ip],
            None => {
                if host.is_empty() || host == "localhost" || host.ends_with(".localhost") {
                    return Err(Block::LocalName);
                }
                resolver
                    .resolve(host, port)
                    .map_err(|_| Block::Unresolved)?
            }
        };
        // One non-public answer blocks the name: an attacker controls which
        // address a client would pick.
        if let Some(&ip) = ips.iter().find(|ip| !is_public_ip(**ip)) {
            return Err(Block::NonPublic { ip });
        }
        ips.first()
            .map(|&ip| SocketAddr::new(ip, port))
            .ok_or(Block::Unresolved)
    }
}
