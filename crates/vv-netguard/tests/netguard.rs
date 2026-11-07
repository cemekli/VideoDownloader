//! Network guard (STATE.md F0.9, ADR-0007, SPEC §20.2, AC-29): the policy
//! with a fake resolver, and the proxy against local servers. A "canary"
//! server stands for a forbidden target: it must never see a connection.

use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use vv_netguard::*;

/// Answers from a table; counts lookups.
struct FakeDns {
    answers: Vec<(&'static str, Vec<&'static str>)>,
    calls: Arc<AtomicUsize>,
}

impl Resolver for FakeDns {
    fn resolve(&self, host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.answers
            .iter()
            .find(|(h, _)| *h == host)
            .map(|(_, ips)| ips.iter().map(|i| i.parse().unwrap()).collect())
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

fn dns() -> FakeDns {
    FakeDns {
        answers: vec![
            ("public.test", vec!["93.184.216.34", "2606:2800:220:1::"]),
            ("rebind.test", vec!["93.184.216.34", "127.0.0.1"]),
            ("metadata.test", vec!["169.254.169.254"]),
            ("mapped.test", vec!["::ffff:10.0.0.1"]),
            ("private6.test", vec!["fd00::1"]),
            ("empty.test", vec![]),
        ],
        calls: Arc::new(AtomicUsize::new(0)),
    }
}

#[test]
fn policy_decisions() {
    let p = Policy::default();
    let d = dns();
    let ok = |h: &str, port| p.vet(&d, h, port);
    assert_eq!(
        ok("public.test", 443),
        Ok("93.184.216.34:443".parse().unwrap())
    );
    assert_eq!(
        ok("PUBLIC.TEST.", 80),
        Ok("93.184.216.34:80".parse().unwrap())
    );
    assert_eq!(
        ok("93.184.216.34", 443),
        Ok("93.184.216.34:443".parse().unwrap())
    );

    let blocked = [
        ("rebind.test", 443, "non_public"), // one bad answer blocks the name
        ("metadata.test", 80, "non_public"),
        ("mapped.test", 443, "non_public"),
        ("private6.test", 443, "non_public"),
        ("empty.test", 443, "unresolved"),
        ("nxdomain.test", 443, "unresolved"),
        ("localhost", 80, "local_name"),
        ("api.LOCALHOST.", 443, "local_name"),
        ("127.0.0.1", 80, "non_public"),
        ("[::1]", 443, "non_public"),
        ("[::ffff:127.0.0.1]", 443, "non_public"),
        ("169.254.169.254", 80, "non_public"),
        ("10.1.2.3", 443, "non_public"),
        ("0.0.0.0", 80, "non_public"),
        ("public.test", 22, "port"),
        ("public.test", 8080, "port"),
    ];
    for (host, port, reason) in blocked {
        let e = p.vet(&d, host, port).unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["reason"], reason, "{host}:{port} -> {e:?}");
    }
    // Port is checked before DNS: a blocked port never causes a lookup.
    let before = d.calls.load(Ordering::SeqCst);
    let _ = p.vet(&d, "public.test", 25);
    assert_eq!(d.calls.load(Ordering::SeqCst), before);
}

/// A server that counts connections and answers every request with `body`.
struct Server {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<String>>>,
}

fn server(body: &'static str) -> Server {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (h, r) = (hits.clone(), requests.clone());
    thread::spawn(move || {
        for c in l.incoming().flatten() {
            h.fetch_add(1, Ordering::SeqCst);
            let r = r.clone();
            thread::spawn(move || {
                let mut c = c;
                let _ = c.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = Vec::new();
                let mut b = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match c.read(&mut b) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&b[..n]),
                    }
                }
                r.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).into_owned());
                let _ = write!(
                    c,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    Server {
        addr,
        hits,
        requests,
    }
}

fn guard(test_targets: Vec<SocketAddr>) -> Guard {
    Guard::start(
        Policy {
            test_targets,
            ..Policy::default()
        },
        Box::new(dns()),
    )
    .unwrap()
}

fn auth_header(g: &Guard) -> String {
    // proxy_url is http://vv:<token>@127.0.0.1:port; build the Basic value from it.
    let u = url::Url::parse(&g.proxy_url()).unwrap();
    let raw = format!("{}:{}", u.username(), u.password().unwrap());
    // RFC 4648 base64, independent of the crate's encoder.
    let t = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in raw.as_bytes().chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() {
                t[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    format!("Proxy-Authorization: Basic {out}\r\n")
}

fn send(g: &Guard, raw: &str) -> String {
    let mut c = TcpStream::connect(g.addr()).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    c.write_all(raw.as_bytes()).unwrap();
    let mut out = Vec::new();
    let _ = c.read_to_end(&mut out);
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn http_requests_are_checked_and_rewritten() {
    let ok = server("hello");
    let canary = server("canary");
    let g = guard(vec![ok.addr]);
    let a = auth_header(&g);

    let r = send(
        &g,
        &format!(
            "GET http://{}/v.mp4?x=1 HTTP/1.1\r\nHost: {}\r\n{a}Connection: keep-alive\r\n\r\n",
            ok.addr, ok.addr
        ),
    );
    assert!(r.starts_with("HTTP/1.1 200") && r.ends_with("hello"), "{r}");
    let seen = ok.requests.lock().unwrap()[0].clone();
    assert!(
        seen.starts_with("GET /v.mp4?x=1 HTTP/1.1\r\n"),
        "origin-form: {seen}"
    );
    assert!(
        !seen.to_ascii_lowercase().contains("proxy-authorization"),
        "{seen}"
    );
    assert!(seen.contains("Connection: close"), "{seen}");

    // Loopback that is not a registered test target: refused, never contacted.
    let r = send(
        &g,
        &format!("GET http://{}/ HTTP/1.1\r\n{a}\r\n", canary.addr),
    );
    assert!(r.starts_with("HTTP/1.1 403"), "{r}");
    let r = send(
        &g,
        &format!(
            "GET http://localhost:{}/ HTTP/1.1\r\n{a}\r\n",
            canary.addr.port()
        ),
    );
    assert!(r.starts_with("HTTP/1.1 403"), "{r}");
    let r = send(
        &g,
        &format!(
            "GET http://[::ffff:127.0.0.1]:{}/ HTTP/1.1\r\n{a}\r\n",
            canary.addr.port()
        ),
    );
    assert!(r.starts_with("HTTP/1.1 403"), "{r}");
    // https in absolute form and credentials in the URL are not proxied.
    for t in [
        "https://public.test/",
        "http://u:p@public.test/",
        "ftp://public.test/",
    ] {
        let r = send(&g, &format!("GET {t} HTTP/1.1\r\n{a}\r\n"));
        assert!(r.starts_with("HTTP/1.1 400"), "{t}: {r}");
    }
    assert_eq!(canary.hits.load(Ordering::SeqCst), 0);
    assert!(g.blocked());
}

#[test]
fn connect_tunnels_are_checked() {
    let ok = server("tunnelled");
    let canary = server("canary");
    let g = guard(vec![ok.addr]);
    let a = auth_header(&g);

    let r = send(
        &g,
        &format!(
            "CONNECT {} HTTP/1.1\r\n{a}\r\nGET / HTTP/1.1\r\nHost: x\r\n\r\n",
            ok.addr
        ),
    );
    assert!(
        r.starts_with("HTTP/1.1 200 Connection Established\r\n\r\nHTTP/1.1 200 OK"),
        "{r}"
    );
    assert!(r.ends_with("tunnelled"));

    for target in [
        canary.addr.to_string(),
        format!("[::1]:{}", canary.addr.port()),
        format!("localhost:{}", canary.addr.port()),
        "rebind.test:443".into(),
        "metadata.test:443".into(),
        "public.test:22".into(),
    ] {
        let r = send(&g, &format!("CONNECT {target} HTTP/1.1\r\n{a}\r\n"));
        assert!(r.starts_with("HTTP/1.1 403"), "{target}: {r}");
    }
    assert_eq!(canary.hits.load(Ordering::SeqCst), 0);
    let blocked = g
        .events()
        .iter()
        .filter(|e| matches!(e.decision, Decision::Blocked { .. }))
        .count();
    assert_eq!(blocked, 6);
}

#[test]
fn only_token_holders_may_use_the_proxy() {
    let ok = server("x");
    let g = guard(vec![ok.addr]);
    for auth in ["", "Proxy-Authorization: Basic dnY6d3Jvbmc=\r\n"] {
        let r = send(&g, &format!("CONNECT {} HTTP/1.1\r\n{auth}\r\n", ok.addr));
        assert!(r.starts_with("HTTP/1.1 407"), "{r}");
        let r = send(
            &g,
            &format!("GET http://{}/ HTTP/1.1\r\n{auth}\r\n", ok.addr),
        );
        assert!(r.starts_with("HTTP/1.1 407"), "{r}");
    }
    assert_eq!(ok.hits.load(Ordering::SeqCst), 0);
    assert!(g
        .events()
        .iter()
        .all(|e| e.decision == Decision::Unauthorized));
    // Two guards never share a token.
    let token = |g: Guard| {
        url::Url::parse(&g.proxy_url())
            .unwrap()
            .password()
            .unwrap()
            .to_owned()
    };
    let (t1, t2) = (token(guard(vec![])), token(guard(vec![])));
    assert_ne!(t1, t2);
    assert_eq!(t1.len(), 36);
}

#[test]
fn a_reused_connection_cannot_reach_a_second_host() {
    let ok = server("first");
    let canary = server("canary");
    let g = guard(vec![ok.addr, canary.addr]);
    let a = auth_header(&g);
    // Second request on the same client connection, to another host.
    let r = send(
        &g,
        &format!(
            "GET http://{}/ HTTP/1.1\r\n{a}\r\nGET http://{}/ HTTP/1.1\r\n{a}\r\n",
            ok.addr, canary.addr
        ),
    );
    assert!(r.contains("first") && !r.contains("canary"), "{r}");
    assert_eq!(canary.hits.load(Ordering::SeqCst), 0);
}

#[test]
fn events_hold_no_paths_or_queries() {
    let ok = server("x");
    let g = guard(vec![ok.addr]);
    let a = auth_header(&g);
    send(
        &g,
        &format!(
            "GET http://{}/secret/path?token=CANARY HTTP/1.1\r\n{a}\r\n",
            ok.addr
        ),
    );
    let json = serde_json::to_string(&g.events()).unwrap();
    assert!(
        !json.contains("secret") && !json.contains("CANARY"),
        "{json}"
    );
    assert!(
        !json.contains(&g.proxy_url()[10..40]),
        "token leaked: {json}"
    );
}
