use crate::NetError;
use crate::codec::{FrameDecoder, FrameEncoder, InboundFrame, SECRET_LEN, Switch};
use crate::pool::BufPool;
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, unbounded};
use mcr_buf::write_varint;
use std::io::Write;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

const EVENT_QUEUE: usize = 4096;
const POOL_SIZE: usize = 256;
const WRITE_BATCH: usize = 64 * 1024;
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

pub type InboundHook = Box<dyn FnMut(i32, &[u8]) -> Option<Switch> + Send>;
pub type FrameTap = Box<dyn FnMut(i32, &[u8]) + Send>;

#[derive(Default)]
pub struct ConnectOptions {
    /// Runs on the reader thread for every decoded frame; a returned [`Switch`] takes effect
    /// before the next frame is parsed.
    pub inbound_hook: Option<InboundHook>,
    /// Observes every decoded frame on the reader thread, in order, before the game thread gets it.
    pub frame_tap: Option<FrameTap>,
    pub connect_timeout: Option<Duration>,
}

impl std::fmt::Debug for ConnectOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectOptions")
            .field("inbound_hook", &self.inbound_hook.is_some())
            .field("frame_tap", &self.frame_tap.is_some())
            .field("connect_timeout", &self.connect_timeout)
            .finish()
    }
}

#[derive(Debug)]
pub enum Event {
    Frame(InboundFrame),
    /// Always the last event.
    Closed(Arc<NetError>),
}

enum Outbound {
    Frame(Vec<u8>),
    Compression(i32),
    Encryption([u8; SECRET_LEN]),
    Shutdown,
}

struct Shared {
    closed: OnceLock<Arc<NetError>>,
    socket: TcpStream,
    armed: AtomicBool,
    armed_key: Mutex<Option<[u8; SECRET_LEN]>>,
}

impl Shared {
    fn fail(&self, err: NetError) -> Arc<NetError> {
        let arc = Arc::new(err);
        let first = self.closed.get_or_init(|| arc).clone();
        let _ = self.socket.shutdown(Shutdown::Both);
        first
    }
}

#[derive(Debug)]
pub struct Connection {
    out_tx: Sender<Outbound>,
    events: Option<Receiver<Event>>,
    shared: Arc<Shared>,
    out_pool: BufPool,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared")
            .field("closed", &self.closed.get())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Outbound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Outbound")
    }
}

impl Connection {
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self, NetError> {
        Self::connect_with(addr, ConnectOptions::default())
    }

    pub fn connect_with(addr: impl ToSocketAddrs, opts: ConnectOptions) -> Result<Self, NetError> {
        let mut last = None;
        for a in addr.to_socket_addrs()? {
            let res = match opts.connect_timeout {
                Some(t) => TcpStream::connect_timeout(&a, t),
                None => TcpStream::connect(a),
            };
            match res {
                Ok(s) => return Self::from_stream(s, opts),
                Err(e) => last = Some(e),
            }
        }
        Err(last
            .unwrap_or_else(|| std::io::Error::other("address resolved to nothing"))
            .into())
    }

    pub fn from_stream(stream: TcpStream, opts: ConnectOptions) -> Result<Self, NetError> {
        stream.set_nodelay(true)?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        let shared = Arc::new(Shared {
            closed: OnceLock::new(),
            socket: stream.try_clone()?,
            armed: AtomicBool::new(false),
            armed_key: Mutex::new(None),
        });
        let (out_tx, out_rx) = unbounded();
        let (ev_tx, ev_rx) = bounded(EVENT_QUEUE);
        let in_pool = BufPool::new(POOL_SIZE);
        let out_pool = BufPool::new(POOL_SIZE);

        let reader = {
            let (shared, stream) = (shared.clone(), stream.try_clone()?);
            std::thread::Builder::new()
                .name("mcr-net-reader".into())
                .spawn(move || {
                    let hooks = ReaderHooks {
                        inbound: opts.inbound_hook,
                        tap: opts.frame_tap,
                    };
                    run_reader(stream, shared, ev_tx, hooks, in_pool)
                })?
        };
        let writer = {
            let (shared, pool) = (shared.clone(), out_pool.clone());
            std::thread::Builder::new()
                .name("mcr-net-writer".into())
                .spawn(move || run_writer(stream, shared, out_rx, pool))?
        };
        Ok(Self {
            out_tx,
            events: Some(ev_rx),
            shared,
            out_pool,
            reader: Some(reader),
            writer: Some(writer),
        })
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events.as_ref()?.try_recv().ok()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.events.as_ref()?.recv_timeout(timeout).ok()
    }

    pub fn close_reason(&self) -> Option<Arc<NetError>> {
        self.shared.closed.get().cloned()
    }

    pub fn send(&self, id: i32, payload: &[u8]) -> Result<(), NetError> {
        let mut body = self.out_pool.get();
        write_varint(&mut body, id);
        body.extend_from_slice(payload);
        self.queue(Outbound::Frame(body))
    }

    /// Outbound only, ordered after every frame already queued. Inbound switches go through
    /// [`ConnectOptions::inbound_hook`].
    pub fn enable_compression(&self, threshold: i32) -> Result<(), NetError> {
        self.queue(Outbound::Compression(threshold))
    }

    /// Arms the reader immediately, so call it only before the peer can send ciphertext;
    /// prefer [`send_and_enable_encryption`](Self::send_and_enable_encryption).
    pub fn enable_encryption(&self, secret: [u8; SECRET_LEN]) -> Result<(), NetError> {
        self.arm(secret);
        self.queue(Outbound::Encryption(secret))
    }

    /// Sends the last plaintext frame, then encrypts everything after it in both directions.
    pub fn send_and_enable_encryption(
        &self,
        id: i32,
        payload: &[u8],
        secret: [u8; SECRET_LEN],
    ) -> Result<(), NetError> {
        self.arm(secret);
        self.send(id, payload)?;
        self.queue(Outbound::Encryption(secret))
    }

    pub fn close(self) {}

    fn arm(&self, secret: [u8; SECRET_LEN]) {
        if let Ok(mut slot) = self.shared.armed_key.lock() {
            *slot = Some(secret);
        }
        self.shared.armed.store(true, Ordering::Release);
    }

    fn queue(&self, msg: Outbound) -> Result<(), NetError> {
        if let Some(reason) = self.shared.closed.get() {
            return Err(NetError::Disconnected(reason.clone()));
        }
        self.out_tx.send(msg).map_err(|_| {
            NetError::Disconnected(
                self.shared
                    .closed
                    .get()
                    .cloned()
                    .unwrap_or_else(|| Arc::new(NetError::LocalClose)),
            )
        })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.events = None;
        let _ = self.out_tx.send(Outbound::Shutdown);
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
        self.shared.fail(NetError::LocalClose);
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
    }
}

struct ReaderHooks {
    inbound: Option<InboundHook>,
    tap: Option<FrameTap>,
}

fn run_reader(
    mut stream: TcpStream,
    shared: Arc<Shared>,
    events: Sender<Event>,
    mut hooks: ReaderHooks,
    pool: BufPool,
) {
    let err = read_loop(&mut stream, &shared, &events, &mut hooks, pool);
    let reason = shared.fail(err);
    let _ = events.send(Event::Closed(reason));
}

fn read_loop(
    stream: &mut TcpStream,
    shared: &Shared,
    events: &Sender<Event>,
    hooks: &mut ReaderHooks,
    pool: BufPool,
) -> NetError {
    let mut dec = FrameDecoder::new(pool);
    loop {
        loop {
            match dec.next_frame() {
                Ok(Some(frame)) => {
                    if let Some(switch) = hooks
                        .inbound
                        .as_mut()
                        .and_then(|h| h(frame.id, &frame.payload))
                    {
                        dec.apply(switch);
                    }
                    if let Some(tap) = hooks.tap.as_mut() {
                        tap(frame.id, &frame.payload);
                    }
                    if events.send(Event::Frame(frame)).is_err() {
                        return NetError::LocalClose;
                    }
                }
                Ok(None) => break,
                Err(e) => return e,
            }
        }
        if let Err(e) = dec.fill(stream) {
            return e;
        }
        if shared.armed.load(Ordering::Acquire)
            && let Ok(mut slot) = shared.armed_key.lock()
            && let Some(key) = slot.take()
        {
            dec.set_encryption(&key);
        }
    }
}

fn run_writer(mut stream: TcpStream, shared: Arc<Shared>, rx: Receiver<Outbound>, pool: BufPool) {
    let err = write_loop(&mut stream, &rx, &pool);
    shared.fail(err);
}

fn write_loop(stream: &mut TcpStream, rx: &Receiver<Outbound>, pool: &BufPool) -> NetError {
    let mut enc = FrameEncoder::new();
    let mut out = Vec::with_capacity(WRITE_BATCH * 2);
    loop {
        let Ok(first) = rx.recv() else {
            return NetError::LocalClose;
        };
        let mut next = Some(first);
        let mut shutdown = false;
        while let Some(msg) = next.take() {
            match msg {
                Outbound::Frame(body) => {
                    let res = enc.encode_body(&body, &mut out);
                    pool.put(body);
                    if let Err(e) = res {
                        return e;
                    }
                }
                Outbound::Compression(t) => enc.set_compression(t),
                Outbound::Encryption(key) => enc.set_encryption(&key, out.len()),
                Outbound::Shutdown => {
                    shutdown = true;
                    break;
                }
            }
            if out.len() >= WRITE_BATCH {
                break;
            }
            match rx.try_recv() {
                Ok(m) => next = Some(m),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
            }
        }
        enc.seal(&mut out);
        if let Err(e) = stream.write_all(&out) {
            return e.into();
        }
        out.clear();
        if shutdown {
            return NetError::LocalClose;
        }
    }
}
