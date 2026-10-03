//! `SessionTransport` — the session-edge channel the serve loop and
//! the client run over (D1 — the rendered stream is visible on a
//! *separate* channel, never folded into the protocol response; the
//! client half is `attach_session`, never an in-process call).
//!
//! Frames are whole JSON-RPC `Json` values — the channel boundary the
//! fixture models is the message, not the byte stream.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hh_wire::json::Json;

/// `SessionTransport` — send/recv whole JSON-RPC frames.
pub trait SessionTransport {
    /// Send one frame (fire-and-forget for notifications; the client
    /// matches responses to requests by `id`).
    fn send(&mut self, frame: Json) -> Result<(), String>;
    /// Receive the next frame (`Ok(None)` on close).
    fn recv(&mut self) -> Result<Option<Json>, String>;
}

/// `MemorySessionTransport` — the in-memory duplex fixture channel.
/// [`channel`] mints a connected pair; each end's `send` lands on the
/// peer's `recv` queue — a real channel boundary (the serve loop and
/// client never share a call frame).
/// The queues are `Arc<Mutex>` — the serve loop and the client run on
/// separate threads (a real channel boundary: two call frames, two
/// owners, one shared wire).
#[derive(Debug)]
pub struct MemorySessionTransport {
    /// Frames this end sends.
    outbox: Arc<Mutex<VecDeque<Json>>>,
    /// Frames this end receives.
    inbox: Arc<Mutex<VecDeque<Json>>>,
    /// This end's close flag (the peer reads it via `peer`).
    closed: Arc<AtomicBool>,
    /// The peer's close flag — `recv`/`send` observe it.
    peer: Arc<AtomicBool>,
}

/// `channel()` — mint a connected `(client_end, server_end)` pair.
pub fn channel() -> (MemorySessionTransport, MemorySessionTransport) {
    let a2b: Arc<Mutex<VecDeque<Json>>> = Arc::new(Mutex::new(VecDeque::new()));
    let b2a: Arc<Mutex<VecDeque<Json>>> = Arc::new(Mutex::new(VecDeque::new()));
    let a_closed = Arc::new(AtomicBool::new(false));
    let b_closed = Arc::new(AtomicBool::new(false));
    (
        MemorySessionTransport {
            outbox: a2b.clone(),
            inbox: b2a.clone(),
            closed: a_closed.clone(),
            peer: b_closed.clone(),
        },
        MemorySessionTransport {
            outbox: b2a,
            inbox: a2b,
            closed: b_closed.clone(),
            peer: a_closed,
        },
    )
}

impl MemorySessionTransport {
    /// Close this end (the peer drains, then sees `None`).
    pub fn close(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// Whether the peer closed its end — `recv` polls this to turn a
    /// closed channel into `None` rather than a busy spin.
    fn peer_closed(&self) -> bool {
        self.peer.load(Ordering::SeqCst)
    }
}

impl Drop for MemorySessionTransport {
    /// Dropping an end closes it — the peer's `recv` observes the flag
    /// and drains to `None` (an undropped-but-gone peer is the one
    /// failure a flag cannot report; explicit `close` is the honest
    /// end signal).
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl SessionTransport for MemorySessionTransport {
    fn send(&mut self, frame: Json) -> Result<(), String> {
        if self.peer_closed() {
            return Err("transport closed".to_string());
        }
        self.outbox
            .lock()
            .map_err(|e| e.to_string())?
            .push_back(frame);
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Json>, String> {
        // Poll with a bounded sleep — `close` is the only end signal;
        // a locked-then-sleep loop keeps the fixture honest without a
        // condvar (records are whole frames, never bytes).
        loop {
            if let Some(f) = self.inbox.lock().map_err(|e| e.to_string())?.pop_front() {
                return Ok(Some(f));
            }
            if self.peer_closed() {
                return Ok(None);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}
