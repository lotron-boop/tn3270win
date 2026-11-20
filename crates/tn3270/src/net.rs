//! TCP connection on a background thread.
//!
//! Connecting and reading happen off the UI thread; each event calls
//! `notify` (the Windows app posts itself a message) and the UI collects
//! events with [`Connection::try_recv`]. Writes are small and done directly.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub enum NetEvent {
    Connected,
    Data(Vec<u8>),
    /// Connection ended; `Some` carries an error message.
    Closed(Option<String>),
}

pub struct Connection {
    rx: Receiver<NetEvent>,
    writer: Arc<Mutex<Option<TcpStream>>>,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

impl Connection {
    pub fn open(host: &str, port: u16, notify: impl Fn() + Send + 'static) -> Connection {
        let (tx, rx) = channel();
        let writer: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));
        let w = Arc::clone(&writer);
        let host = host.to_string();
        std::thread::spawn(move || {
            let send = |ev| {
                let _ = tx.send(ev);
                notify();
            };
            let mut stream = match connect(&host, port) {
                Ok(s) => s,
                Err(e) => return send(NetEvent::Closed(Some(e))),
            };
            let _ = stream.set_nodelay(true); // keystrokes go out immediately
            match stream.try_clone() {
                Ok(clone) => *w.lock().unwrap() = Some(clone),
                Err(e) => return send(NetEvent::Closed(Some(e.to_string()))),
            }
            send(NetEvent::Connected);
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => return send(NetEvent::Closed(None)),
                    Ok(n) => send(NetEvent::Data(buf[..n].to_vec())),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        let closed_by_us = w.lock().map(|g| g.is_none()).unwrap_or(true);
                        return send(NetEvent::Closed(if closed_by_us { None } else { Some(e.to_string()) }));
                    }
                }
            }
        });
        Connection { rx, writer }
    }

    pub fn try_recv(&self) -> Option<NetEvent> {
        self.rx.try_recv().ok()
    }

    pub fn send(&self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        match self.writer.lock().unwrap().as_mut() {
            Some(s) => s.write_all(bytes),
            None => Err(io::Error::new(io::ErrorKind::NotConnected, "not connected")),
        }
    }

    pub fn close(&self) {
        if let Some(s) = self.writer.lock().unwrap().take() {
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

    /// A fake host: classic TN3270 negotiation, a logon screen, then it
    /// reads the operator's Enter.
    #[test]
    fn end_to_end_with_fake_host() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(&[IAC, DO, OPT_EOR, IAC, WILL, OPT_EOR, IAC, DO, OPT_BINARY, IAC, WILL, OPT_BINARY]).unwrap();
            let mut rec = vec![0xF5, WCC_KEYBOARD_RESTORE, ORDER_SF, FA_PROTECTED];
            rec.extend(CodePage::Cp037.encode("LOGON ===>"));
            rec.extend_from_slice(&[ORDER_SF, 0x00, ORDER_IC]);
            rec.extend_from_slice(&[IAC, EOR]);
            s.write_all(&rec).unwrap();
            // Read until the client's record ends with IAC EOR.
            let mut got = Vec::new();
            let mut buf = [0u8; 256];
            while !(got.contains(&0x7D) && got.ends_with(&[IAC, EOR])) {
                let n = s.read(&mut buf).unwrap();
                if n == 0 { break; }
                got.extend_from_slice(&buf[..n]);
            }
            got
        });

        let conn = Connection::open("127.0.0.1", port, || {});
        assert_eq!(next(&conn), NetEvent::Connected);
        let mut session = Session::new(CodePage::Cp037, None);
        while !session.screen.row_text(0).contains("LOGON") {
            if let NetEvent::Data(d) = next(&conn) {
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

        let got = host.join().unwrap();
        let tso = CodePage::Cp037.encode("TSO");
        assert!(got.windows(3).any(|w| w == tso.as_slice()), "host received TSO");
        conn.close();
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
