//! TCP (optionally wrapped, e.g. in TLS) on a background thread.
//!
//! One I/O thread owns the connection: it connects, runs the optional
//! wrapper (the TLS handshake), then alternates between sending queued
//! writes and reading. Each event calls `notify` (the Windows app posts
//! itself a message) and the UI collects events with
//! [`Connection::try_recv`].
//!
//! A single owner matters for TLS: the encryption state can't be shared
//! between a reading and a writing thread.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Something that carries bytes to and from the host.
pub trait Transport: Send {
    /// Like `Read::read`. A timeout must come back as `TimedOut` or
    /// `WouldBlock` without losing buffered data.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn write_all(&mut self, data: &[u8]) -> io::Result<()>;
    /// Set the read timeout on the socket this transport reads from. (On
    /// Windows each cloned socket handle has its own timeout, so it must be
    /// the very handle used for reading.)
    fn set_read_timeout(&mut self, t: Option<Duration>) -> io::Result<()>;
}

impl Transport for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Read::read(self, buf)
    }
    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        Write::write_all(self, data)
    }
    fn set_read_timeout(&mut self, t: Option<Duration>) -> io::Result<()> {
        TcpStream::set_read_timeout(self, t)
    }
}

/// The user's answer to a certificate question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustAnswer {
    /// Trust this certificate for this host from now on.
    Always,
    /// Connect this time only.
    Once,
    /// Don't connect.
    Reject,
}

/// A certificate Windows doesn't trust: the UI asks the user and replies.
#[derive(Debug)]
pub struct TrustPrompt {
    pub host: String,
    pub port: u16,
    pub subject: String,
    pub issuer: String,
    pub expires: String,
    /// SHA-256, as `AB:CD:...`.
    pub fingerprint: String,
    /// Why Windows didn't accept it.
    pub problem: String,
    /// A different certificate was trusted for this host before.
    pub changed: bool,
    pub reply: Sender<TrustAnswer>,
}

#[derive(Debug)]
pub enum NetEvent {
    Connected,
    /// The connection is encrypted (description such as "TLS").
    Secured(String),
    Prompt(TrustPrompt),
    Data(Vec<u8>),
    /// Connection ended; `Some` carries an error message.
    Closed(Option<String>),
}

/// Runs on the I/O thread after TCP connects: wraps the stream (e.g. TLS
/// handshake). It can send events, such as a [`NetEvent::Prompt`].
pub type Wrapper = Box<dyn FnOnce(TcpStream, &dyn Fn(NetEvent)) -> Result<Box<dyn Transport>, String> + Send>;

enum Cmd {
    Write(Vec<u8>),
    Close,
}

pub struct Connection {
    rx: Receiver<NetEvent>,
    cmd: Sender<Cmd>,
    /// A handle to the socket, so close() can interrupt a blocked read.
    socket: Arc<Mutex<Option<TcpStream>>>,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a read waits before checking for queued writes.
const POLL: Duration = Duration::from_millis(15);

impl Connection {
    /// Plain TCP.
    pub fn open(host: &str, port: u16, notify: impl Fn() + Send + 'static) -> Connection {
        Connection::open_with(host, port, None, notify)
    }

    pub fn open_with(host: &str, port: u16, wrapper: Option<Wrapper>, notify: impl Fn() + Send + 'static) -> Connection {
        let (tx, rx) = channel();
        let (cmd_tx, cmd_rx) = channel::<Cmd>();
        let socket: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));
        let sock = Arc::clone(&socket);
        let host = host.to_string();
        std::thread::spawn(move || {
            let send = |ev| {
                let _ = tx.send(ev);
                notify();
            };
            let closed = |e: String| Some(e);
            let stream = match connect(&host, port) {
                Ok(s) => s,
                Err(e) => return send(NetEvent::Closed(closed(e))),
            };
            let _ = stream.set_nodelay(true); // keystrokes go out immediately
            // A second handle to the socket, so close() can shut it down
            // while this thread is reading.
            match stream.try_clone() {
                Ok(shut) => *sock.lock().unwrap() = Some(shut),
                Err(e) => return send(NetEvent::Closed(closed(e.to_string()))),
            }
            send(NetEvent::Connected);

            let mut transport: Box<dyn Transport> = match wrapper {
                Some(wrap) => match wrap(stream, &send) {
                    Ok(t) => t,
                    Err(e) => return send(NetEvent::Closed(closed(e))),
                },
                None => Box::new(stream),
            };
            // Reads wake up regularly so queued writes go out promptly.
            if transport.set_read_timeout(Some(POLL)).is_err() {
                return send(NetEvent::Closed(closed("could not set socket timeout".into())));
            }

            let mut buf = vec![0u8; 16 * 1024];
            loop {
                loop {
                    match cmd_rx.try_recv() {
                        Ok(Cmd::Write(d)) => {
                            if let Err(e) = transport.write_all(&d) {
                                return send(NetEvent::Closed(closed(format!("Send failed: {e}"))));
                            }
                        }
                        Ok(Cmd::Close) | Err(TryRecvError::Disconnected) => return,
                        Err(TryRecvError::Empty) => break,
                    }
                }
                match transport.read(&mut buf) {
                    Ok(0) => return send(NetEvent::Closed(None)),
                    Ok(n) => send(NetEvent::Data(buf[..n].to_vec())),
                    Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                        | io::ErrorKind::Interrupted) => {}
                    Err(e) => {
                        let closed_by_us = sock.lock().map(|g| g.is_none()).unwrap_or(true);
                        return send(NetEvent::Closed(if closed_by_us { None } else { Some(e.to_string()) }));
                    }
                }
            }
        });
        Connection { rx, cmd: cmd_tx, socket }
    }

    pub fn try_recv(&self) -> Option<NetEvent> {
        self.rx.try_recv().ok()
    }

    /// Queue bytes for the host.
    pub fn send(&self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.cmd.send(Cmd::Write(bytes.to_vec()))
            .map_err(|_| io::Error::new(io::ErrorKind::NotConnected, "not connected"))
    }

    pub fn close(&self) {
        let _ = self.cmd.send(Cmd::Close);
        if let Some(s) = self.socket.lock().unwrap().take() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.close();
    }
}

fn connect(host: &str, port: u16) -> Result<TcpStream, String> {
    let addrs: Vec<_> = (host, port).to_socket_addrs()
        .map_err(|e| format!("Can't find host {host}: {e}"))?
        .collect();
    let mut last = format!("Can't find host {host}");
    for a in addrs {
        match TcpStream::connect_timeout(&a, CONNECT_TIMEOUT) {
            Ok(s) => return Ok(s),
            Err(e) => last = format!("Can't connect to {host} port {port}: {e}"),
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::*;
    use crate::screen::Aid;
    use crate::session::{Session, SessionState};
    use crate::telnet::*;
    use crate::CodePage;
    use std::net::TcpListener;

    fn next(c: &Connection) -> NetEvent {
        for _ in 0..500 {
            if let Some(e) = c.try_recv() {
                return e;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out");
    }

    fn fake_host(listener: TcpListener, xor: u8) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut out = vec![IAC, DO, OPT_EOR, IAC, WILL, OPT_EOR, IAC, DO, OPT_BINARY, IAC, WILL, OPT_BINARY];
            out.extend_from_slice(&[0xF5, WCC_KEYBOARD_RESTORE, ORDER_SF, FA_PROTECTED]);
            out.extend(CodePage::Cp037.encode("LOGON ===>"));
            out.extend_from_slice(&[ORDER_SF, 0x00, ORDER_IC, IAC, EOR]);
            let out: Vec<u8> = out.iter().map(|b| b ^ xor).collect();
            Write::write_all(&mut s, &out).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 256];
            while !(got.contains(&0x7D) && got.ends_with(&[IAC, EOR])) {
                let n = Read::read(&mut s, &mut buf).unwrap();
                if n == 0 { break; }
                got.extend(buf[..n].iter().map(|b| b ^ xor));
            }
            got
        })
    }

    fn log_on(conn: &Connection) {
        let mut session = Session::new(CodePage::Cp037, None);
        while !session.screen.row_text(0).contains("LOGON") {
            if let NetEvent::Data(d) = next(conn) {
                session.receive(&d);
                conn.send(&session.take_output()).unwrap();
            }
        }
        assert!(matches!(session.state, SessionState::Ready { .. }));
        for c in "TSO".chars() {
            session.screen.type_char(c).unwrap();
        }
        session.aid(Aid::Enter).unwrap();
        conn.send(&session.take_output()).unwrap();
    }

    /// A fake host: classic TN3270 negotiation, a logon screen, then it
    /// reads the operator's Enter.
    #[test]
    fn end_to_end_with_fake_host() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = fake_host(listener, 0);
        let conn = Connection::open("127.0.0.1", port, || {});
        assert!(matches!(next(&conn), NetEvent::Connected));
        log_on(&conn);
        let got = host.join().unwrap();
        let tso = CodePage::Cp037.encode("TSO");
        assert!(got.windows(3).any(|w| w == tso.as_slice()), "host received TSO");
        conn.close();
    }

    /// A toy "encryption" wrapper (XOR) exercises the wrapper path the TLS
    /// code uses: events from the wrapper, a prompt answered by the UI side,
    /// buffered reads with timeouts, and writes from the I/O thread.
    struct Xor(TcpStream);
    impl Transport for Xor {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = Read::read(&mut self.0, buf)?;
            buf[..n].iter_mut().for_each(|b| *b ^= 0x5A);
            Ok(n)
        }
        fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
            let x: Vec<u8> = data.iter().map(|b| b ^ 0x5A).collect();
            Write::write_all(&mut self.0, &x)
        }
        fn set_read_timeout(&mut self, t: Option<Duration>) -> io::Result<()> {
            self.0.set_read_timeout(t)
        }
    }

    #[test]
    fn wrapper_with_trust_prompt() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = fake_host(listener, 0x5A);
        let wrapper: Wrapper = Box::new(move |stream, emit| {
            let (tx, rx) = channel();
            emit(NetEvent::Prompt(TrustPrompt {
                host: "127.0.0.1".into(), port, subject: "CN=test".into(), issuer: "CN=test".into(),
                expires: "2036-01-01".into(), fingerprint: "AA".into(), problem: "self-signed".into(),
                changed: false, reply: tx,
            }));
            match rx.recv() {
                Ok(TrustAnswer::Once) | Ok(TrustAnswer::Always) => {}
                _ => return Err("Certificate not trusted".into()),
            }
            emit(NetEvent::Secured("XOR".into()));
            Ok(Box::new(Xor(stream)))
        });
        let conn = Connection::open_with("127.0.0.1", port, Some(wrapper), || {});
        assert!(matches!(next(&conn), NetEvent::Connected));
        match next(&conn) {
            NetEvent::Prompt(p) => p.reply.send(TrustAnswer::Once).unwrap(),
            other => panic!("expected a prompt, got {other:?}"),
        }
        assert!(matches!(next(&conn), NetEvent::Secured(_)));
        log_on(&conn);
        let got = host.join().unwrap();
        let tso = CodePage::Cp037.encode("TSO");
        assert!(got.windows(3).any(|w| w == tso.as_slice()), "host received TSO through the wrapper");
    }

    #[test]
    fn rejected_certificate_closes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let _host = std::thread::spawn(move || { let _ = listener.accept(); });
        let wrapper: Wrapper = Box::new(|_stream, emit| {
            let (tx, rx) = channel();
            emit(NetEvent::Prompt(TrustPrompt {
                host: String::new(), port: 0, subject: String::new(), issuer: String::new(),
                expires: String::new(), fingerprint: String::new(), problem: String::new(),
                changed: true, reply: tx,
            }));
            match rx.recv() {
                Ok(TrustAnswer::Reject) | Err(_) => Err("Certificate not trusted".into()),
                Ok(_) => unreachable!(),
            }
        });
        let conn = Connection::open_with("127.0.0.1", port, Some(wrapper), || {});
        assert!(matches!(next(&conn), NetEvent::Connected));
        if let NetEvent::Prompt(p) = next(&conn) {
            p.reply.send(TrustAnswer::Reject).unwrap();
        }
        assert!(matches!(next(&conn), NetEvent::Closed(Some(m)) if m.contains("not trusted")));
    }

    #[test]
    fn connection_refused_is_reported() {
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        }; // listener dropped: nothing listens there now
        let conn = Connection::open("127.0.0.1", port, || {});
        assert!(matches!(next(&conn), NetEvent::Closed(Some(_))));
    }
}
