use mcr_net::{
    BufPool, ConnectOptions, Connection, Event, FrameDecoder, FrameEncoder, NetError, Switch,
};
use std::io::Write;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::time::Duration;

const HELLO: i32 = 0x10;
const KEY: i32 = 0x20;
const SET_COMPRESSION: i32 = 0x03;
const BIG: i32 = 0x30;
const SMALL: i32 = 0x31;
const ECHO: i32 = 0x40;
const THRESHOLD: i32 = 256;
const SECRET: [u8; 16] = *b"0123456789abcdef";
const WAIT: Duration = Duration::from_secs(5);

fn pattern(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 7) as u8).collect()
}

fn read_frame(dec: &mut FrameDecoder, s: &mut TcpStream) -> (i32, Vec<u8>) {
    loop {
        if let Some(f) = dec.next_frame().unwrap() {
            return (f.id, f.payload.to_vec());
        }
        dec.fill(s).unwrap();
    }
}

fn send(enc: &mut FrameEncoder, s: &mut TcpStream, frames: &[(i32, &[u8])]) {
    let mut out = Vec::new();
    for (id, p) in frames {
        enc.encode(*id, p, &mut out).unwrap();
    }
    enc.seal(&mut out);
    s.write_all(&out).unwrap();
}

fn next_frame(conn: &Connection) -> (i32, Vec<u8>) {
    match conn.recv_timeout(WAIT).expect("timed out") {
        Event::Frame(f) => (f.id, f.payload.to_vec()),
        Event::Closed(e) => panic!("closed: {e}"),
    }
}

fn fake_server(listener: TcpListener) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut dec = FrameDecoder::new(BufPool::new(4));
        let mut enc = FrameEncoder::new();
        send(&mut enc, &mut s, &[(HELLO, b"hi")]);

        let (id, payload) = read_frame(&mut dec, &mut s);
        assert_eq!((id, payload.as_slice()), (KEY, &b"secret"[..]));
        dec.apply(Switch::Encryption(SECRET));
        enc.set_encryption(&SECRET, 0);

        let big = pattern(5000);
        let small = pattern(10);
        send(
            &mut enc,
            &mut s,
            &[(SET_COMPRESSION, &THRESHOLD.to_be_bytes())],
        );
        enc.set_compression(THRESHOLD);
        dec.apply(Switch::Compression(THRESHOLD));
        send(&mut enc, &mut s, &[(BIG, &big), (SMALL, &small)]);

        let (id, payload) = read_frame(&mut dec, &mut s);
        assert_eq!((id, payload), (BIG, big.clone()));
        let (id, payload) = read_frame(&mut dec, &mut s);
        assert_eq!((id, payload), (SMALL, small));
        send(&mut enc, &mut s, &[(ECHO, b"done")]);
        s.shutdown(Shutdown::Both).unwrap();
    })
}

#[test]
fn encryption_and_compression_enabled_midstream() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = fake_server(listener);

    let opts = ConnectOptions {
        inbound_hook: Some(Box::new(|id, payload| {
            (id == SET_COMPRESSION)
                .then(|| Switch::Compression(i32::from_be_bytes(payload.try_into().unwrap())))
        })),
        ..Default::default()
    };
    let conn = Connection::connect_with(addr, opts).unwrap();

    assert_eq!(next_frame(&conn), (HELLO, b"hi".to_vec()));
    conn.send_and_enable_encryption(KEY, b"secret", SECRET)
        .unwrap();

    let (id, payload) = next_frame(&conn);
    assert_eq!(id, SET_COMPRESSION);
    conn.enable_compression(i32::from_be_bytes(payload.try_into().unwrap()))
        .unwrap();
    conn.send(BIG, &pattern(5000)).unwrap();
    conn.send(SMALL, &pattern(10)).unwrap();

    assert_eq!(next_frame(&conn), (BIG, pattern(5000)));
    assert_eq!(next_frame(&conn), (SMALL, pattern(10)));
    assert_eq!(next_frame(&conn), (ECHO, b"done".to_vec()));

    match conn.recv_timeout(WAIT).expect("closed event") {
        Event::Closed(reason) => assert!(matches!(*reason, NetError::Eof), "{reason}"),
        Event::Frame(_) => panic!("unexpected frame"),
    }
    assert!(matches!(conn.send(1, b""), Err(NetError::Disconnected(_))));
    assert!(conn.recv_timeout(Duration::from_millis(50)).is_none());
    server.join().unwrap();
}

#[test]
fn corrupt_stream_reports_reason() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.write_all(&[0x80, 0x80, 0x80, 0x01]).unwrap();
        std::thread::sleep(Duration::from_millis(200));
    });
    let conn = Connection::connect(addr).unwrap();
    match conn.recv_timeout(WAIT).expect("closed event") {
        Event::Closed(reason) => assert!(matches!(*reason, NetError::LengthTooWide)),
        Event::Frame(_) => panic!("unexpected frame"),
    }
    server.join().unwrap();
}

#[test]
fn local_close_flushes_queued_frames_and_joins() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut dec = FrameDecoder::new(BufPool::new(4));
        let mut seen = Vec::new();
        for _ in 0..100 {
            seen.push(read_frame(&mut dec, &mut s).0);
        }
        seen
    });
    let conn = Connection::connect(addr).unwrap();
    for i in 0..100 {
        conn.send(i, &pattern(100)).unwrap();
    }
    conn.close();
    assert_eq!(server.join().unwrap(), (0..100).collect::<Vec<_>>());
}
