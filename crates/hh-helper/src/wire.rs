//! The `hh-helper/1` transport — newline-delimited canonical JSON over the
//! bridged `AF_UNIX` channel (OQ-161). Canonical JSON never embeds a raw
//! `\n` inside a string (escaped), so one line is one object — no length
//! prefix needed, and a truncated final line is a clean EOF signal. Decode
//! is `hh_wire::json::parse` — the one canonicalizer (CC1, never a second
//! JSON reader).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use hh_wire::json::{self, Json};

/// A wire I/O failure (the transport plane).
#[derive(Debug)]
pub struct WireError(pub String);

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wire: {}", self.0)
    }
}

impl std::error::Error for WireError {}

impl From<std::io::Error> for WireError {
    fn from(e: std::io::Error) -> Self {
        WireError(e.to_string())
    }
}

/// A line-channel over the session socket (one canonical JSON object per
/// line). Concrete over `UnixStream` — the channel *is* the bridged socket.
pub struct Channel {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Channel {
    /// Wrap a connected socket.
    pub fn new(stream: UnixStream) -> std::io::Result<Channel> {
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Channel {
            reader,
            writer: stream,
        })
    }

    /// Send one object (canonical bytes + `\n`, flushed).
    pub fn send(&mut self, j: &Json) -> Result<(), WireError> {
        self.writer.write_all(j.to_canonical_string().as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }

    /// Receive one object — `Ok(None)` on clean EOF (the peer closed after
    /// the last full line); a partial tail line is `Err` (truncated frame).
    pub fn recv(&mut self) -> Result<Option<Json>, WireError> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        if !line.ends_with('\n') {
            return Err(WireError("truncated frame at EOF".into()));
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            return self.recv();
        }
        json::parse(trimmed)
            .map(Some)
            .map_err(|e| WireError(format!("frame decode: {e}")))
    }
}
