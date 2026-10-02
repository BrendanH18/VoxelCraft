//! Bounded, versioned JSON-lines transport for agents controlling hosted players.
//! Network threads never read or mutate gameplay state; the game thread answers requests.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};
use serde_json::{Value, json};

pub const VERSION: u64 = 1;
pub const MAX_LINE: usize = 4096;
const MAX_CONNECTIONS: usize = 16;

pub struct Request {
    pub player: String,
    pub command: String,
    pub reply: Sender<Value>,
}

pub struct Host {
    pub address: SocketAddr,
    pub requests: Receiver<Request>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

/// Reads one bounded line without allocating according to an untrusted message length.
pub fn read_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    read_bounded_line(reader, MAX_LINE)
}

/// Client observations can be larger than requests, but remain bounded.
pub fn read_response(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    read_bounded_line(reader, 256 * 1024)
}

fn read_bounded_line(reader: &mut impl BufRead, limit: usize) -> io::Result<Option<String>> {
    let mut bytes = Vec::new();
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return if bytes.is_empty() { Ok(None) } else { Err(io::Error::other("incomplete JSON line")) };
        }
        let n = buf.iter().position(|&b| b == b'\n').map_or(buf.len(), |n| n + 1);
        if bytes.len() + n > limit {
            return Err(io::Error::other("message exceeds 4096 bytes"));
        }
        let complete = buf[n - 1] == b'\n';
        bytes.extend_from_slice(&buf[..n]);
        reader.consume(n);
        if complete {
            return String::from_utf8(bytes).map(Some).map_err(io::Error::other);
        }
    }
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 24 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl Host {
    /// Bind explicitly. Non-loopback listeners require a shared token; loopback is the default.
    pub fn bind(address: SocketAddr, token: Option<String>) -> io::Result<Self> {
        if !address.ip().is_loopback() && token.as_ref().is_none_or(|t| t.len() < 16) {
            return Err(io::Error::other("LAN control requires --agent-token with at least 16 characters"));
        }
        let listener = TcpListener::bind(address)?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let (send, requests) = bounded(64);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let active = Arc::new(AtomicUsize::new(0));
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                            continue;
                        }
                        active.fetch_add(1, Ordering::Relaxed);
                        let (send, token, active) = (send.clone(), token.clone(), active.clone());
                        thread::spawn(move || {
                            let _ = serve(stream, &send, token.as_deref());
                            active.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        });
        Ok(Self { address, requests, stop, thread: Some(thread) })
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, send: &Sender<Request>, token: Option<&str>) -> io::Result<()> {
    // Accepted sockets inherit nonblocking mode on macOS. Worker threads use bounded blocking I/O.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    while let Some(line) = read_line(&mut reader)? {
        let result = (|| {
            let value: Value = serde_json::from_str(&line).map_err(|_| "invalid JSON")?;
            if value["version"].as_u64() != Some(VERSION) {
                return Err("protocol version mismatch (expected 1)");
            }
            if token.is_some_and(|t| value["token"].as_str() != Some(t)) {
                return Err("invalid agent token");
            }
            let name = value["player"].as_str().filter(|n| valid_name(n)).ok_or("invalid player name")?;
            let command = value["command"].as_str().filter(|c| c.len() <= 1024).ok_or("invalid command")?;
            let (reply, response) = bounded(1);
            send.try_send(Request { player: name.into(), command: command.into(), reply }).map_err(|_| "host busy")?;
            response.recv_timeout(Duration::from_secs(15)).map_err(|_| "host did not answer")
        })();
        let value = result.unwrap_or_else(|e| json!({"ok":false,"error":e,"version":VERSION}));
        writeln!(stream, "{value}")?;
        stream.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_and_identity() {
        assert!(valid_name("Builder_2"));
        assert!(!valid_name("../player"));
        assert!(!valid_name(""));
        assert!(read_line(&mut &b"hello\n"[..]).unwrap().is_some());
        assert!(read_line(&mut &vec![b'x'; MAX_LINE + 1][..]).is_err());
        assert!(read_line(&mut &b"partial"[..]).is_err());
        assert!(Host::bind("0.0.0.0:0".parse().unwrap(), None).is_err());
    }

    #[test]
    fn loopback_protocol_and_bad_handshake() {
        let host = Host::bind("127.0.0.1:0".parse().unwrap(), Some("secret".into())).unwrap();
        let mut stream = TcpStream::connect(host.address).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        writeln!(stream, "{}", json!({"version":99,"player":"bot","command":"observe"})).unwrap();
        let bad: Value = serde_json::from_str(&read_line(&mut reader).unwrap().unwrap()).unwrap();
        assert_eq!(bad["ok"], false);
        writeln!(stream, "{}", json!({"version":1,"token":"secret","player":"bot","command":"observe"})).unwrap();
        let req = host.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(req.player, "bot");
        req.reply.send(json!({"ok":true,"tick":42})).unwrap();
        let good: Value = serde_json::from_str(&read_line(&mut reader).unwrap().unwrap()).unwrap();
        assert_eq!(good["tick"], 42);
    }
}
