//! The `plugin_abi/1` framed channel (§8.4 §3 — the envelope
//! `{protocol_version, schema_hash, seq, payload}` over a length-prefixed
//! byte frame). The channel owns the *session* sequence counters: every sent
//! envelope is stamped `seq = tx + 1`; every received envelope must carry
//! `seq = rx + 1` exactly (the `seq`-based resume basis — a gap, a replay or
//! a regression is a `SeqViolation`, which desyncs the session and forces
//! `detached`; V2/V5). Framing, sizing and ordering live here; *content*
//! screening (direction, authority, bindings) lives in [`crate::screen`].

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use hh_embed_schema::plugin_abi::{AbiEnvelope, AbiError, AbiPayload};
use hh_wire::json::Json;

/// The frame ceiling (4 MiB). An oversized frame is refused before its body
/// is read — a hostile variant cannot make the host buffer unboundedly.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// Channel-layer failures. `Screened` wraps a content-level refusal the
/// codec surfaced (`AbiEnvelope::from_json`'s `AbiError`); the seq family is
/// terminal for the session (the stream cannot be re-synchronised — V2).
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelError {
    /// The transport failed (`io` detail — a died plugin surfaces as `Eof`).
    Io(String),
    /// A read deadline elapsed (`InvocationTimeout`'s transport half).
    Timeout,
    /// The peer hung up cleanly (session end or crash — the caller
    /// distinguishes by state).
    Eof,
    /// A frame exceeded [`MAX_FRAME_BYTES`].
    Oversized(usize),
    /// The envelope failed strict decode (schema violation).
    Schema(AbiError),
    /// `seq ≠ rx + 1` — gap, replay or regression (terminal desync).
    SeqViolation {
        /// The expected next seq.
        expected: i64,
        /// The seq actually received.
        got: i64,
    },
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelError::Io(e) => write!(f, "channel io: {e}"),
            ChannelError::Timeout => write!(f, "channel deadline"),
            ChannelError::Eof => write!(f, "channel eof"),
            ChannelError::Oversized(n) => write!(f, "frame {n}B > cap"),
            ChannelError::Schema(e) => write!(f, "schema: {}", e.as_str()),
            ChannelError::SeqViolation { expected, got } => {
                write!(f, "seq {got} != expected {expected}")
            }
        }
    }
}
impl std::error::Error for ChannelError {}

/// The raw byte-pipe half of a channel — one framed pipe in each direction
/// is modelled as one `FrameIo` (a socket) or one queue pair (memory).
pub trait FrameIo: Send {
    /// Write one frame (the codec adds the length prefix itself).
    fn write_frame(&mut self, bytes: &[u8]) -> Result<(), ChannelError>;
    /// Read one frame; `deadline` bounds the wait (`None` = unbounded —
    /// used only outside invocation windows).
    fn read_frame(&mut self, deadline: Option<Duration>) -> Result<Vec<u8>, ChannelError>;
    /// Close the pipe (best effort).
    fn close(&mut self);
}

/// `AbiChannel` — the sequenced envelope channel over one `FrameIo`.
pub struct AbiChannel {
    io: Box<dyn FrameIo>,
    /// Sent count (the next envelope's `seq - 1`).
    tx: i64,
    /// Received count (the last accepted `seq`).
    rx: i64,
}

impl AbiChannel {
    /// Wrap a transport.
    pub fn new(io: Box<dyn FrameIo>) -> AbiChannel {
        AbiChannel { io, tx: 0, rx: 0 }
    }

    /// The next seq a `send` will stamp.
    pub fn next_tx(&self) -> i64 {
        self.tx + 1
    }

    /// The last accepted inbound seq.
    pub fn rx(&self) -> i64 {
        self.rx
    }

    /// Send `payload` — the envelope is stamped here (`seq = tx + 1`,
    /// `protocol_version`/`schema_hash` asserted by the caller's payload;
    /// the headers are *set* by the channel so no caller can desync them).
    pub fn send(&mut self, schema_hash: &str, payload: AbiPayload) -> Result<i64, ChannelError> {
        self.tx += 1;
        let env = AbiEnvelope {
            protocol_version: hh_embed_schema::plugin_abi::PLUGIN_ABI_MAJOR,
            schema_hash: schema_hash.to_string(),
            seq: self.tx,
            payload,
        };
        let bytes = env.to_json().to_canonical_string().into_bytes();
        self.io.write_frame(&bytes)?;
        Ok(self.tx)
    }

    /// Receive the next envelope — strict decode, exact `seq` advance.
    ///
    /// The headers and `seq` are checked *before* the payload decodes: a
    /// well-framed message whose payload fails the closed sum (an `allow`
    /// verdict, an unknown member) is refused as `Schema(..)` with `rx`
    /// already advanced — a content violation refuses the *message* and
    /// keeps the session in sync (AC-4: the run proceeds on the class
    /// fallback). Only framing/seq faults desync the channel, and a
    /// desynced stream stays broken (the caller detaches rather than
    /// resynchronising by guesswork).
    pub fn recv(&mut self, deadline: Option<Duration>) -> Result<AbiEnvelope, ChannelError> {
        let bytes = self.io.read_frame(deadline)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| ChannelError::Schema(AbiError::SchemaViolation))?;
        let j = hh_wire::json::parse(&text)
            .map_err(|_| ChannelError::Schema(AbiError::SchemaViolation))?;
        let Json::Obj(m) = &j else {
            return Err(ChannelError::Schema(AbiError::SchemaViolation));
        };
        for k in m.keys() {
            if !matches!(
                k.as_str(),
                "protocol_version" | "schema_hash" | "seq" | "payload"
            ) {
                return Err(ChannelError::Schema(AbiError::SchemaViolation));
            }
        }
        let seq = match m.get("seq") {
            Some(Json::Int(n)) => *n,
            _ => return Err(ChannelError::Schema(AbiError::SchemaViolation)),
        };
        let expected = self.rx + 1;
        if seq != expected {
            return Err(ChannelError::SeqViolation { expected, got: seq });
        }
        // seq is authoritative — the message exists on the stream even when
        // its payload is refused.
        self.rx = seq;
        let env = AbiEnvelope::from_json(&j).map_err(ChannelError::Schema)?;
        Ok(env)
    }

    /// Send a pre-formed envelope JSON verbatim (the conformance fixture's
    /// forgery knob — a hostile variant can emit any bytes it likes; the
    /// host's job is to refuse them). If the envelope carries a `seq` ahead
    /// of `tx` the local counter advances to it so subsequent well-formed
    /// sends stay consistent for the peer's checker.
    pub fn send_json(&mut self, env_json: &Json) -> Result<i64, ChannelError> {
        if let Some(Json::Int(n)) = env_json.get("seq") {
            if *n > self.tx {
                self.tx = *n;
            }
        }
        let bytes = env_json.to_canonical_string().into_bytes();
        self.io.write_frame(&bytes)?;
        Ok(self.tx)
    }

    /// Write raw frame bytes (the oversized/malformed probes — bypasses
    /// the codec entirely).
    pub fn send_frame_bytes(&mut self, bytes: &[u8]) -> Result<(), ChannelError> {
        self.io.write_frame(bytes)
    }

    /// Close the transport.
    pub fn close(&mut self) {
        self.io.close();
    }
}

// ── socket transport ──────────────────────────────────────────────────────────

/// The subprocess channel — a unix socket the *variant* connects back to
/// (the host listens; the path is the one member of the spawned process's
/// `unix_sockets.allow` — the socket *is* the ABI channel, and the only
/// network-class syscall the confined process may make).
pub struct SocketIo {
    stream: UnixStream,
}

impl SocketIo {
    /// Wrap a connected stream. Force blocking mode — on macOS/BSD
    /// `accept` inherits the listener's `O_NONBLOCK`, and deadlines are
    /// applied per-read via `set_read_timeout` anyway.
    pub fn new(stream: UnixStream) -> SocketIo {
        let _ = stream.set_nonblocking(false);
        SocketIo { stream }
    }

    fn read_exact_deadline(
        &mut self,
        buf: &mut [u8],
        deadline: Option<Duration>,
    ) -> Result<(), ChannelError> {
        let to = deadline.map(|d| d.max(Duration::from_millis(1)));
        self.stream
            .set_read_timeout(to)
            .map_err(|e| ChannelError::Io(e.to_string()))?;
        match self.stream.read_exact(buf) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Err(ChannelError::Eof),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Err(ChannelError::Timeout)
            }
            Err(e) => Err(ChannelError::Io(e.to_string())),
        }
    }
}

impl FrameIo for SocketIo {
    fn write_frame(&mut self, bytes: &[u8]) -> Result<(), ChannelError> {
        let len = (bytes.len() as u32).to_be_bytes();
        self.stream
            .write_all(&len)
            .and_then(|()| self.stream.write_all(bytes))
            .and_then(|()| self.stream.flush())
            .map_err(|e| ChannelError::Io(e.to_string()))
    }

    fn read_frame(&mut self, deadline: Option<Duration>) -> Result<Vec<u8>, ChannelError> {
        let mut hdr = [0u8; 4];
        self.read_exact_deadline(&mut hdr, deadline)?;
        let len = u32::from_be_bytes(hdr) as usize;
        if len > MAX_FRAME_BYTES {
            return Err(ChannelError::Oversized(len));
        }
        let mut buf = vec![0u8; len];
        self.read_exact_deadline(&mut buf, deadline)?;
        Ok(buf)
    }

    fn close(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

// ── memory transport (protocol-suite lanes) ───────────────────────────────────

struct MemState {
    queue: VecDeque<Vec<u8>>,
    closed: bool,
}

/// A shared memory pipe end.
pub struct MemIo {
    /// Frames this end reads.
    inbound: Arc<(Mutex<MemState>, Condvar)>,
    /// Frames this end writes.
    outbound: Arc<(Mutex<MemState>, Condvar)>,
}

impl MemIo {
    /// A connected pair — `(a, b)` where `a`'s writes are `b`'s reads.
    pub fn pair() -> (MemIo, MemIo) {
        let ab = Arc::new((
            Mutex::new(MemState {
                queue: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let ba = Arc::new((
            Mutex::new(MemState {
                queue: VecDeque::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        (
            MemIo {
                inbound: ba.clone(),
                outbound: ab.clone(),
            },
            MemIo {
                inbound: ab,
                outbound: ba,
            },
        )
    }
}

/// A dropped end is a dead peer — mark both directions closed so the
/// other side sees `Eof`, never a hang (the crash fixture relies on this).
impl Drop for MemIo {
    fn drop(&mut self) {
        self.close();
    }
}

impl FrameIo for MemIo {
    fn write_frame(&mut self, bytes: &[u8]) -> Result<(), ChannelError> {
        let (lock, cv) = &*self.outbound;
        let mut st = lock.lock().map_err(|_| ChannelError::Io("poison".into()))?;
        if st.closed {
            return Err(ChannelError::Eof);
        }
        st.queue.push_back(bytes.to_vec());
        cv.notify_one();
        Ok(())
    }

    fn read_frame(&mut self, deadline: Option<Duration>) -> Result<Vec<u8>, ChannelError> {
        let (lock, cv) = &*self.inbound;
        let mut st = lock.lock().map_err(|_| ChannelError::Io("poison".into()))?;
        let end = deadline.map(|d| Instant::now() + d);
        loop {
            if let Some(f) = st.queue.pop_front() {
                if f.len() > MAX_FRAME_BYTES {
                    return Err(ChannelError::Oversized(f.len()));
                }
                return Ok(f);
            }
            if st.closed {
                return Err(ChannelError::Eof);
            }
            let Some(end) = end else {
                st = cv.wait(st).map_err(|_| ChannelError::Io("poison".into()))?;
                continue;
            };
            let now = Instant::now();
            if now >= end {
                return Err(ChannelError::Timeout);
            }
            let (s, _t) = cv
                .wait_timeout(st, end - now)
                .map_err(|_| ChannelError::Io("poison".into()))?;
            st = s;
        }
    }

    fn close(&mut self) {
        for arc in [&self.inbound, &self.outbound] {
            let (lock, cv) = &**arc;
            if let Ok(mut st) = lock.lock() {
                st.closed = true;
            }
            cv.notify_all();
        }
    }
}
