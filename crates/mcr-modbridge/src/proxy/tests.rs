use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

fn local(port: u16) -> ProxyConfig {
    ProxyConfig {
        hosts: vec!["127.0.0.1".into()],
        ports: vec![port],
        idle: Duration::from_millis(400),
        allow_local: true,
        ..ProxyConfig::default()
    }
}

fn client(proxy: &HttpProxy) -> TcpStream {
    let s = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy.port())).unwrap();
    s.set_read_timeout(Some(WAIT)).unwrap();
    s
}

fn read_all(s: &mut TcpStream) -> String {
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    String::from_utf8_lossy(&out).into_owned()
}

fn status_of(proxy: &HttpProxy, request: &[u8]) -> String {
    let mut s = client(proxy);
    s.write_all(request).unwrap();
    let text = read_all(&mut s);
    text.lines().next().unwrap_or_default().to_owned()
}

fn echo_server() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut buf = [0u8; 256];
                while let Ok(n) = s.read(&mut buf) {
                    if n == 0 || s.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
        }
    });
    port
}

/// Answers one request with `ok` and hands its head to the test.
fn http_server() -> (u16, mpsc::Receiver<String>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") && s.read(&mut byte).unwrap() == 1 {
            head.push(byte[0]);
        }
        tx.send(String::from_utf8(head).unwrap()).unwrap();
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .unwrap();
    });
    (port, rx)
}

#[test]
fn connect_tunnels_to_an_allowed_host() {
    let port = echo_server();
    let proxy = HttpProxy::start(local(port)).unwrap();
    let mut s = client(&proxy);
    write!(
        s,
        "CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: x\r\n\r\nearly"
    )
    .unwrap();
    let mut buf = [0u8; 64];
    let mut got = Vec::new();
    while !got.ends_with(b"early") {
        let n = s.read(&mut buf).unwrap();
        assert_ne!(
            n,
            0,
            "tunnel closed early: {:?}",
            String::from_utf8_lossy(&got)
        );
        got.extend_from_slice(&buf[..n]);
    }
    assert!(got.starts_with(b"HTTP/1.1 200 "));
    s.write_all(b"ping").unwrap();
    let mut pong = [0u8; 4];
    s.read_exact(&mut pong).unwrap();
    assert_eq!(&pong, b"ping");
}

#[test]
fn absolute_form_requests_are_rewritten_and_closed() {
    let (port, heads) = http_server();
    let proxy = HttpProxy::start(local(port)).unwrap();
    let mut s = client(&proxy);
    write!(
        s,
        "GET http://127.0.0.1:{port}/skin?x=1 HTTP/1.1\r\nHost: evil.internal\r\n\
         Proxy-Connection: keep-alive\r\nConnection: keep-alive\r\nAccept: image/png\r\n\r\n"
    )
    .unwrap();
    let head = heads.recv_timeout(WAIT).unwrap();
    assert!(head.starts_with("GET /skin?x=1 HTTP/1.1\r\n"), "{head}");
    assert!(
        head.contains(&format!("Host: 127.0.0.1:{port}\r\n")),
        "{head}"
    );
    assert!(head.contains("Accept: image/png\r\n"), "{head}");
    assert!(head.ends_with("Connection: close\r\n\r\n"), "{head}");
    assert!(!head.contains("evil.internal"), "{head}");
    assert!(!head.to_ascii_lowercase().contains("keep-alive"), "{head}");
    assert!(read_all(&mut s).ends_with("\r\n\r\nok"));
}

#[test]
fn hosts_and_ports_off_the_list_are_forbidden() {
    let port = echo_server();
    let proxy = HttpProxy::start(local(port)).unwrap();
    for request in [
        "CONNECT example.com:443 HTTP/1.1\r\n\r\n".to_owned(),
        format!(
            "CONNECT 127.0.0.1:{} HTTP/1.1\r\n\r\n",
            port.wrapping_add(1)
        ),
        "GET http://textures.minecraft.net/x HTTP/1.1\r\n\r\n".to_owned(),
    ] {
        assert_eq!(
            status_of(&proxy, request.as_bytes()),
            "HTTP/1.1 403 Forbidden",
            "{request}"
        );
    }
}

#[test]
fn allowed_names_resolving_locally_are_forbidden() {
    let port = echo_server();
    let proxy = HttpProxy::start(ProxyConfig {
        allow_local: false,
        ..local(port)
    })
    .unwrap();
    let request = format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n");
    assert_eq!(
        status_of(&proxy, request.as_bytes()),
        "HTTP/1.1 403 Forbidden"
    );
}

#[test]
fn malformed_requests_get_400() {
    let proxy = HttpProxy::start(local(80)).unwrap();
    let cases: &[&[u8]] = &[
        b"garbage\r\n\r\n",
        b"CONNECT 127.0.0.1:80\r\n\r\n",
        b"CONNECT 127.0.0.1:80 HTTP/2.0\r\n\r\n",
        b"connect 127.0.0.1:80 HTTP/1.1\r\n\r\n",
        b"CONNECT  127.0.0.1:80 HTTP/1.1\r\n\r\n",
        b"CONNECT 127.0.0.1 HTTP/1.1\r\n\r\n",
        b"CONNECT 127.0.0.1:0 HTTP/1.1\r\n\r\n",
        b"CONNECT 127.0.0.1:99999 HTTP/1.1\r\n\r\n",
        b"CONNECT [::1]:80 HTTP/1.1\r\n\r\n",
        b"CONNECT a..b:80 HTTP/1.1\r\n\r\n",
        b"GET https://127.0.0.1/ HTTP/1.1\r\n\r\n",
        b"GET /relative HTTP/1.1\r\n\r\n",
        b"GET http://user@127.0.0.1/ HTTP/1.1\r\n\r\n",
        b"GET http://127.0.0.1:80@x/ HTTP/1.1\r\n\r\n",
        b"GET http://127.0.0.1/ HTTP/1.1\r\nX: a\nInjected: b\r\n\r\n",
        b"GET http://127.0.0.1/ HTTP/1.1\r\nX: a\r\n folded\r\n\r\n",
        b"GET http://127.0.0.1/ HTTP/1.1\r\nBad Name: a\r\n\r\n",
        b"GET http://127.0.0.1/ HTTP/1.1\r\nNoColon\r\n\r\n",
        b"GET http://127.0.0.1/\x01 HTTP/1.1\r\n\r\n",
        b"GET http://127.0.0.1/ HTTP/1.1\r\nX: \xff\r\n\r\n",
        b"GET http://127.0.0.1/ HTTP/1.1\r\nX: a\x00b\r\n\r\n",
    ];
    for case in cases {
        assert_eq!(
            status_of(&proxy, case),
            "HTTP/1.1 400 Bad Request",
            "{:?}",
            String::from_utf8_lossy(case)
        );
    }
}

#[test]
fn oversized_heads_get_431() {
    let proxy = HttpProxy::start(ProxyConfig {
        max_header: 256,
        ..local(80)
    })
    .unwrap();
    let mut s = client(&proxy);
    let _ = s.write_all(b"GET http://127.0.0.1/ HTTP/1.1\r\nX: ");
    let _ = s.write_all(&[b'a'; 4096]);
    let text = read_all(&mut s);
    assert!(text.starts_with("HTTP/1.1 431 "), "{text}");
}

#[test]
fn slow_heads_time_out() {
    let proxy = HttpProxy::start(local(80)).unwrap();
    let mut s = client(&proxy);
    let start = Instant::now();
    let mut closed = false;
    for _ in 0..50 {
        if s.write_all(b"X").is_err() {
            closed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        s.set_nonblocking(true).unwrap();
        let mut buf = [0u8; 64];
        let r = s.read(&mut buf);
        s.set_nonblocking(false).unwrap();
        if r.is_ok() {
            closed = true;
            break;
        }
    }
    assert!(closed, "a trickled head kept the connection open");
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn silent_clients_and_idle_tunnels_are_closed() {
    let port = echo_server();
    let proxy = HttpProxy::start(local(port)).unwrap();
    let start = Instant::now();
    let mut silent = client(&proxy);
    let text = read_all(&mut silent);
    assert!(
        text.is_empty() || text.starts_with("HTTP/1.1 408 "),
        "{text}"
    );
    let mut tunnel = client(&proxy);
    write!(tunnel, "CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n").unwrap();
    assert!(read_all(&mut tunnel).starts_with("HTTP/1.1 200 "));
    assert!(start.elapsed() < WAIT);
}

#[test]
fn connections_beyond_the_limit_get_503() {
    let proxy = HttpProxy::start(ProxyConfig {
        max_connections: 2,
        idle: Duration::from_secs(5),
        ..local(80)
    })
    .unwrap();
    let held: Vec<TcpStream> = (0..2).map(|_| client(&proxy)).collect();
    std::thread::sleep(Duration::from_millis(100));
    let mut extra = client(&proxy);
    assert!(read_all(&mut extra).starts_with("HTTP/1.1 503 "));
    drop(held);
    let deadline = Instant::now() + WAIT;
    loop {
        let mut s = client(&proxy);
        s.write_all(b"CONNECT example.com:443 HTTP/1.1\r\n\r\n")
            .unwrap();
        if read_all(&mut s).starts_with("HTTP/1.1 403 ") {
            break;
        }
        assert!(Instant::now() < deadline, "slots were not released");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn allow_list_matches_names_and_wildcards() {
    let list = AllowList::new(&[
        "Textures.Minecraft.NET.".into(),
        "*.modrinth.com".into(),
        "*.com".into(),
        "bad host".into(),
        "*.".into(),
    ]);
    assert!(list.permits("textures.minecraft.net"));
    assert!(list.permits("api.modrinth.com"));
    assert!(list.permits("a.b.modrinth.com"));
    assert!(!list.permits("modrinth.com"));
    assert!(!list.permits("evilmodrinth.com"));
    assert!(!list.permits("example.com"));
    assert_eq!(list.exact.len(), 1);
    assert_eq!(list.suffixes, vec![".modrinth.com".to_owned()]);
}

#[test]
fn only_global_addresses_count_as_remote() {
    for ip in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.1.1",
        "100.64.0.1",
        "0.0.0.0",
        "255.255.255.255",
        "::1",
        "::",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
    ] {
        assert!(!is_global(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["1.1.1.1", "13.107.42.14", "2606:4700::1111"] {
        assert!(is_global(ip.parse().unwrap()), "{ip}");
    }
}

#[test]
fn dropping_the_proxy_stops_accepting() {
    let proxy = HttpProxy::start(local(80)).unwrap();
    let port = proxy.port();
    drop(proxy);
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
}
