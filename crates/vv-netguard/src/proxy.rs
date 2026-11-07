//! The local proxy: HTTP `CONNECT` tunnels (HTTPS) and absolute-form HTTP
//! requests, each checked by [`Policy::vet`] before any outbound connection.
//!
//! Blocking std threads, one per connection: the proxy serves one app's
//! tool processes, not the public.

use crate::policy::{Block, Policy, Resolver};
use serde::Serialize;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Upper bound for a request head; anything longer is refused.
const MAX_HEAD: usize = 16 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// User name in the proxy URL; the token is the password.
pub const PROXY_USER: &str = "vv";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Allowed {
        addr: SocketAddr,
    },
    Blocked {
        block: Block,
    },
    /// Missing or wrong token: another local process tried to use the proxy.
    Unauthorized,
}

/// One proxied connection attempt. Host and port only: no path, no query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Event {
    /// `connect` (HTTPS tunnel) or `http`.
    pub method: &'static str,
    pub host: String,
    pub port: u16,
    #[serde(flatten)]
    pub decision: Decision,
}

struct Shared {
    policy: Policy,
    resolver: Box<dyn Resolver>,
    auth: String,
    events: Mutex<Vec<Event>>,
    stop: AtomicBool,
}

/// A running proxy on `127.0.0.1:<random port>`. Stops when dropped.
pub struct Guard {
    addr: SocketAddr,
    token: String,
    shared: Arc<Shared>,
    accept: Option<thread::JoinHandle<()>>,
}

impl Guard {
    pub fn start(policy: Policy, resolver: Box<dyn Resolver>) -> io::Result<Guard> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let addr = listener.local_addr()?;
        let mut raw = [0u8; 18];
        getrandom::fill(&mut raw).map_err(|e| io::Error::other(e.to_string()))?;
        // URL-safe so it can sit in the proxy URL without escaping.
        let token: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        let auth = format!(
            "Basic {}",
            base64(format!("{PROXY_USER}:{token}").as_bytes())
        );
        let shared = Arc::new(Shared {
            policy,
            resolver,
            auth,
            events: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
        });
        let s = Arc::clone(&shared);
        let accept = thread::spawn(move || {
            for conn in listener.incoming() {
                if s.stop.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(c) = conn {
                    let s = Arc::clone(&s);
                    thread::spawn(move || {
                        let _ = handle(&s, c);
                    });
                }
            }
        });
        Ok(Guard {
            addr,
            token,
            shared,
            accept: Some(accept),
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The value for yt-dlp's `--proxy`. Holds the token: never log it.
    pub fn proxy_url(&self) -> String {
        format!("http://{PROXY_USER}:{}@{}", self.token, self.addr)
    }

    pub fn events(&self) -> Vec<Event> {
        self.shared.events.lock().unwrap().clone()
    }

    /// True when any connection was refused by the policy: the job's error is
    /// then `URL_BLOCKED`, whatever yt-dlp printed.
    pub fn blocked(&self) -> bool {
        self.events()
            .iter()
            .any(|e| matches!(e.decision, Decision::Blocked { .. }))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
        if let Some(t) = self.accept.take() {
            let _ = t.join();
        }
    }
}

struct Head {
    method: String,
    target: String,
    version: String,
    headers: Vec<(String, String)>,
}

fn read_head(c: &mut TcpStream) -> io::Result<(Head, Vec<u8>)> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    let end = loop {
        let n = c.read(&mut chunk)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > MAX_HEAD {
            return Err(io::ErrorKind::InvalidData.into());
        }
    };
    let rest = buf.split_off(end);
    let text =
        std::str::from_utf8(&buf).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, target, version) = match (first.next(), first.next(), first.next(), first.next()) {
        (Some(m), Some(t), Some(v), None) if !m.is_empty() && !t.is_empty() => {
            (m.to_owned(), t.to_owned(), v.to_owned())
        }
        _ => return Err(io::ErrorKind::InvalidData.into()),
    };
    let headers = lines
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Ok((
        Head {
            method,
            target,
            version,
            headers,
        },
        rest,
    ))
}

fn reply(c: &mut TcpStream, status: &str, extra: &str) {
    let _ = write!(
        c,
        "HTTP/1.1 {status}\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let _ = c.flush();
}

fn record(s: &Shared, method: &'static str, host: &str, port: u16, decision: Decision) {
    s.events.lock().unwrap().push(Event {
        method,
        host: host.to_owned(),
        port,
        decision,
    });
}

/// `host:port`, `[v6]:port`.
fn split_authority(a: &str) -> Option<(String, u16)> {
    let (h, p) = a.rsplit_once(':')?;
    let port = p.parse().ok()?;
    (!h.is_empty()).then(|| (h.to_owned(), port))
}

fn handle(s: &Shared, mut client: TcpStream) -> io::Result<()> {
    client.set_read_timeout(Some(HEAD_TIMEOUT))?;
    let (head, early) = match read_head(&mut client) {
        Ok(h) => h,
        Err(_) => {
            reply(&mut client, "400 Bad Request", "");
            return Ok(());
        }
    };
    let authorised = head
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("proxy-authorization") && *v == s.auth);

    let connect = head.method.eq_ignore_ascii_case("CONNECT");
    let (method, host, port, path) = if connect {
        match split_authority(&head.target) {
            Some((h, p)) => ("connect", h, p, None),
            None => return bad(s, &mut client, "connect"),
        }
    } else {
        match url::Url::parse(&head.target) {
            Ok(u) if u.scheme() == "http" && u.username().is_empty() && u.password().is_none() => {
                let host = match u.host_str() {
                    Some(h) => h.to_owned(),
                    None => return bad(s, &mut client, "http"),
                };
                let mut path = u.path().to_owned();
                if let Some(q) = u.query() {
                    path.push('?');
                    path.push_str(q);
                }
                (
                    "http",
                    host,
                    u.port_or_known_default().unwrap_or(80),
                    Some(path),
                )
            }
            _ => return bad(s, &mut client, "http"),
        }
    };
    if !authorised {
        record(s, method, &host, port, Decision::Unauthorized);
        reply(
            &mut client,
            "407 Proxy Authentication Required",
            "Proxy-Authenticate: Basic realm=\"videovault\"\r\n",
        );
        return Ok(());
    }
    let addr = match s.policy.vet(s.resolver.as_ref(), &host, port) {
        Ok(a) => a,
        Err(block) => {
            record(s, method, &host, port, Decision::Blocked { block });
            reply(
                &mut client,
                "403 Forbidden",
                "X-VideoVault-Block: policy\r\n",
            );
            return Ok(());
        }
    };
    let mut upstream = match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
        Ok(u) => u,
        Err(_) => {
            record(s, method, &host, port, Decision::Allowed { addr });
            reply(&mut client, "502 Bad Gateway", "");
            return Ok(());
        }
    };
    record(s, method, &host, port, Decision::Allowed { addr });
    client.set_read_timeout(None)?;

    if connect {
        client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
        upstream.write_all(&early)?;
    } else {
        // Origin-form request; proxy headers dropped; one request per
        // connection so a reused client connection cannot reach another host.
        let mut out = format!("{} {} {}\r\n", head.method, path.unwrap(), head.version);
        for (k, v) in &head.headers {
            let k_l = k.to_ascii_lowercase();
            if k_l.starts_with("proxy-") || k_l == "connection" || k_l == "keep-alive" {
                continue;
            }
            out.push_str(&format!("{k}: {v}\r\n"));
        }
        out.push_str("Connection: close\r\n\r\n");
        upstream.write_all(out.as_bytes())?;
        upstream.write_all(&early)?;
    }
    pipe(client, upstream)
}

fn bad(s: &Shared, c: &mut TcpStream, method: &'static str) -> io::Result<()> {
    record(
        s,
        method,
        "",
        0,
        Decision::Blocked {
            block: Block::BadRequest,
        },
    );
    reply(c, "400 Bad Request", "");
    Ok(())
}

fn pipe(client: TcpStream, upstream: TcpStream) -> io::Result<()> {
    let (mut c_r, mut u_w) = (client.try_clone()?, upstream.try_clone()?);
    let up = thread::spawn(move || {
        let _ = io::copy(&mut c_r, &mut u_w);
        let _ = u_w.shutdown(Shutdown::Write);
    });
    let (mut u_r, mut c_w) = (upstream, client);
    let _ = io::copy(&mut u_r, &mut c_w);
    let _ = c_w.shutdown(Shutdown::Both);
    let _ = u_r.shutdown(Shutdown::Both);
    let _ = up.join();
    Ok(())
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in input.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_rfc4648_vectors() {
        for (i, o) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::base64(i.as_bytes()), o);
        }
    }
}
