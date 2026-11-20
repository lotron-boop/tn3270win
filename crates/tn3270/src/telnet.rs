//! Telnet negotiation and record framing for TN3270 / TN3270E.
//!
//! The client offers TN3270E (RFC 2355) if the host asks for it, requesting
//! no optional functions, and falls back to classic TN3270 (RFC 1576:
//! TERMINAL-TYPE, BINARY and END-OF-RECORD) if the host refuses.

pub const IAC: u8 = 255;
pub const DONT: u8 = 254;
pub const DO: u8 = 253;
pub const WONT: u8 = 252;
pub const WILL: u8 = 251;
pub const SB: u8 = 250;
pub const IP: u8 = 244;
pub const SE: u8 = 240;
pub const EOR: u8 = 239;

pub const OPT_BINARY: u8 = 0;
pub const OPT_TTYPE: u8 = 24;
pub const OPT_EOR: u8 = 25;
pub const OPT_TN3270E: u8 = 40;

const TTYPE_IS: u8 = 0;
const TTYPE_SEND: u8 = 1;

// TN3270E subnegotiation codes
const E_CONNECT: u8 = 1;
const E_DEVICE_TYPE: u8 = 2;
const E_FUNCTIONS: u8 = 3;
const E_IS: u8 = 4;
const E_REASON: u8 = 5;
const E_REJECT: u8 = 6;
const E_REQUEST: u8 = 7;
const E_SEND: u8 = 8;

/// TN3270E header data types.
pub const DT_3270_DATA: u8 = 0;
pub const DT_UNBIND: u8 = 4;
pub const DT_SSCP_LU_DATA: u8 = 7;

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// A 3270 data-stream record (command + data).
    Data(Vec<u8>),
    /// TN3270E SSCP-LU data: plain EBCDIC text from the network (e.g. USS).
    SscpLu(Vec<u8>),
    /// TN3270E: the host ended the LU-LU session.
    Unbind,
    /// Negotiation finished; 3270 mode is active.
    Ready { tn3270e: bool, device_type: String, lu: Option<String> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parse {
    Data,
    Iac,
    Opt(u8),
    Sb,
    SbIac,
}

pub struct Telnet {
    parse: Parse,
    record: Vec<u8>,
    sb: Vec<u8>,
    out: Vec<u8>,
    /// Options we have agreed to (WILL) / asked the host to (DO).
    will: [bool; 256],
    doo: [bool; 256],
    pub terminal_type: String,
    /// LU (device) name to ask for, if any.
    pub requested_lu: Option<String>,
    tn3270e: bool,
    ready: bool,
    device_type: String,
    lu: Option<String>,
}

impl Telnet {
    pub fn new(terminal_type: &str, requested_lu: Option<String>) -> Telnet {
        Telnet {
            parse: Parse::Data,
            record: Vec::new(),
            sb: Vec::new(),
            out: Vec::new(),
            will: [false; 256],
            doo: [false; 256],
            terminal_type: terminal_type.to_string(),
            requested_lu,
            tn3270e: false,
            ready: false,
            device_type: terminal_type.to_string(),
            lu: None,
        }
    }

    pub fn is_tn3270e(&self) -> bool {
        self.tn3270e
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Bytes to send to the host (negotiation replies).
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// Process bytes from the host.
    pub fn receive(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        for &b in bytes {
            match self.parse {
                Parse::Data => {
                    if b == IAC { self.parse = Parse::Iac } else { self.record.push(b) }
                }
                Parse::Iac => {
                    self.parse = Parse::Data;
                    match b {
                        IAC => self.record.push(IAC),
                        EOR => {
                            let rec = std::mem::take(&mut self.record);
                            self.record_done(rec, &mut events);
                        }
                        SB => {
                            self.sb.clear();
                            self.parse = Parse::Sb;
                        }
                        WILL | WONT | DO | DONT => self.parse = Parse::Opt(b),
                        _ => {}
                    }
                }
                Parse::Opt(cmd) => {
                    self.parse = Parse::Data;
                    self.option(cmd, b, &mut events);
                }
                Parse::Sb => {
                    if b == IAC { self.parse = Parse::SbIac } else { self.sb.push(b) }
                }
                Parse::SbIac => match b {
                    SE => {
                        self.parse = Parse::Data;
                        let sb = std::mem::take(&mut self.sb);
                        self.subneg(&sb, &mut events);
                    }
                    IAC => {
                        self.sb.push(IAC);
                        self.parse = Parse::Sb;
                    }
                    _ => self.parse = Parse::Data,
                },
            }
        }
        events
    }

    /// Frame an outbound 3270 record (escaping IAC, adding the TN3270E
    /// header in TN3270E mode, ending with IAC EOR).
    pub fn encode_record(&self, data_type: u8, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(data.len() + 8);
        if self.tn3270e {
            v.extend_from_slice(&[data_type, 0, 0, 0, 0]);
        }
        for &b in data {
            v.push(b);
            if b == IAC {
                v.push(IAC);
            }
        }
        v.extend_from_slice(&[IAC, EOR]);
        v
    }

    /// Telnet Interrupt Process: how ATTN is sent.
    pub fn interrupt(&self) -> Vec<u8> {
        vec![IAC, IP]
    }

    fn send(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    fn option(&mut self, cmd: u8, opt: u8, events: &mut Vec<Event>) {
        let i = opt as usize;
        match cmd {
            DO => {
                let ok = matches!(opt, OPT_TTYPE | OPT_EOR | OPT_BINARY | OPT_TN3270E);
                if ok && !self.will[i] {
                    self.will[i] = true;
                    self.send(&[IAC, WILL, opt]);
                } else if !ok {
                    self.send(&[IAC, WONT, opt]);
                }
            }
            DONT => {
                if self.will[i] {
                    self.will[i] = false;
                    self.send(&[IAC, WONT, opt]);
                }
                if opt == OPT_TN3270E {
                    self.tn3270e = false;
                }
            }
            WILL => {
                let ok = matches!(opt, OPT_EOR | OPT_BINARY);
                if ok && !self.doo[i] {
                    self.doo[i] = true;
                    self.send(&[IAC, DO, opt]);
                } else if !ok {
                    self.send(&[IAC, DONT, opt]);
                }
            }
            WONT if self.doo[i] => {
                self.doo[i] = false;
                self.send(&[IAC, DONT, opt]);
            }
            _ => {}
        }
        self.check_classic_ready(events);
    }

    /// Classic TN3270 is ready once binary and EOR are agreed both ways.
    fn check_classic_ready(&mut self, events: &mut Vec<Event>) {
        if self.ready || self.tn3270e || self.will[OPT_TN3270E as usize] {
            return;
        }
        let both = |o: u8| self.will[o as usize] && self.doo[o as usize];
        if both(OPT_BINARY) && both(OPT_EOR) {
            self.ready = true;
            events.push(Event::Ready { tn3270e: false, device_type: self.terminal_type.clone(), lu: None });
        }
    }

    fn subneg(&mut self, sb: &[u8], events: &mut Vec<Event>) {
        match sb {
            [OPT_TTYPE, TTYPE_SEND, ..] => {
                let mut v = vec![IAC, SB, OPT_TTYPE, TTYPE_IS];
                v.extend_from_slice(self.terminal_type.as_bytes());
                v.extend_from_slice(&[IAC, SE]);
                self.send(&v);
            }
            [OPT_TN3270E, E_SEND, E_DEVICE_TYPE, ..] => {
                let mut v = vec![IAC, SB, OPT_TN3270E, E_DEVICE_TYPE, E_REQUEST];
                v.extend_from_slice(self.terminal_type.as_bytes());
                if let Some(lu) = &self.requested_lu {
                    v.push(E_CONNECT);
                    v.extend_from_slice(lu.as_bytes());
                }
                v.extend_from_slice(&[IAC, SE]);
                self.send(&v);
            }
            [OPT_TN3270E, E_DEVICE_TYPE, E_IS, rest @ ..] => {
                let (dev, lu) = match rest.iter().position(|&b| b == E_CONNECT) {
                    Some(p) => (&rest[..p], Some(&rest[p + 1..])),
                    None => (rest, None),
                };
                self.device_type = String::from_utf8_lossy(dev).into_owned();
                self.lu = lu.map(|l| String::from_utf8_lossy(l).into_owned());
                // Ask for no optional functions (plain 3270 data only).
                self.send(&[IAC, SB, OPT_TN3270E, E_FUNCTIONS, E_REQUEST, IAC, SE]);
            }
            [OPT_TN3270E, E_DEVICE_TYPE, E_REJECT, ..] => {
                // Fall back to classic TN3270.
                self.will[OPT_TN3270E as usize] = false;
                self.send(&[IAC, WONT, OPT_TN3270E]);
            }
            [OPT_TN3270E, E_FUNCTIONS, E_REQUEST, list @ ..] => {
                if list.is_empty() {
                    self.send(&[IAC, SB, OPT_TN3270E, E_FUNCTIONS, E_IS, IAC, SE]);
                    self.tn3270e_ready(events);
                } else {
                    // We support none of the optional functions: counter
                    // with the (empty) subset we accept.
                    self.send(&[IAC, SB, OPT_TN3270E, E_FUNCTIONS, E_REQUEST, IAC, SE]);
                }
            }
            [OPT_TN3270E, E_FUNCTIONS, E_IS, ..] => self.tn3270e_ready(events),
            [OPT_TN3270E, E_REASON, ..] => {}
            _ => {}
        }
    }

    fn tn3270e_ready(&mut self, events: &mut Vec<Event>) {
        if !self.ready {
            self.tn3270e = true;
            self.ready = true;
            events.push(Event::Ready { tn3270e: true, device_type: self.device_type.clone(), lu: self.lu.clone() });
        }
    }

    fn record_done(&mut self, rec: Vec<u8>, events: &mut Vec<Event>) {
        if !self.tn3270e {
            if !rec.is_empty() {
                events.push(Event::Data(rec));
            }
            return;
        }
        if rec.len() < 5 {
            return;
        }
        let body = rec[5..].to_vec();
        match rec[0] {
            DT_3270_DATA => events.push(Event::Data(body)),
            DT_SSCP_LU_DATA => events.push(Event::SscpLu(body)),
            DT_UNBIND => events.push(Event::Unbind),
            _ => {} // SCS, NVT, BIND-IMAGE...: not negotiated
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TT: &str = "IBM-3278-2-E";

    #[test]
    fn tn3270e_negotiation() {
        let mut t = Telnet::new(TT, None);
        assert!(t.receive(&[IAC, DO, OPT_TN3270E]).is_empty());
        assert_eq!(t.take_output(), vec![IAC, WILL, OPT_TN3270E]);

        t.receive(&[IAC, SB, OPT_TN3270E, E_SEND, E_DEVICE_TYPE, IAC, SE]);
        let mut want = vec![IAC, SB, OPT_TN3270E, E_DEVICE_TYPE, E_REQUEST];
        want.extend_from_slice(TT.as_bytes());
        want.extend_from_slice(&[IAC, SE]);
        assert_eq!(t.take_output(), want);

        let mut is = vec![IAC, SB, OPT_TN3270E, E_DEVICE_TYPE, E_IS];
        is.extend_from_slice(TT.as_bytes());
        is.push(E_CONNECT);
        is.extend_from_slice(b"LU00042");
        is.extend_from_slice(&[IAC, SE]);
        t.receive(&is);
        assert_eq!(t.take_output(), vec![IAC, SB, OPT_TN3270E, E_FUNCTIONS, E_REQUEST, IAC, SE]);

        let ev = t.receive(&[IAC, SB, OPT_TN3270E, E_FUNCTIONS, E_IS, IAC, SE]);
        assert_eq!(ev, vec![Event::Ready { tn3270e: true, device_type: TT.into(), lu: Some("LU00042".into()) }]);

        // A 3270-DATA record with header, and an escaped 0xFF.
        let ev = t.receive(&[0, 0, 0, 0, 1, 0xF5, 0xC3, 0xFF, 0xFF, IAC, EOR]);
        assert_eq!(ev, vec![Event::Data(vec![0xF5, 0xC3, 0xFF])]);
        assert_eq!(t.encode_record(DT_3270_DATA, &[0x7D, 0xFF]), vec![0, 0, 0, 0, 0, 0x7D, 0xFF, 0xFF, IAC, EOR]);
    }

    #[test]
    fn host_counter_requests_functions() {
        let mut t = Telnet::new(TT, None);
        t.receive(&[IAC, DO, OPT_TN3270E]);
        t.take_output();
        t.receive(&[IAC, SB, OPT_TN3270E, E_FUNCTIONS, E_REQUEST, 2, 4, IAC, SE]);
        assert_eq!(t.take_output(), vec![IAC, SB, OPT_TN3270E, E_FUNCTIONS, E_REQUEST, IAC, SE]);
        let ev = t.receive(&[IAC, SB, OPT_TN3270E, E_FUNCTIONS, E_REQUEST, IAC, SE]);
        assert!(matches!(ev[0], Event::Ready { tn3270e: true, .. }));
    }

    #[test]
    fn classic_fallback() {
        let mut t = Telnet::new(TT, None);
        t.receive(&[IAC, DO, OPT_TN3270E]);
        t.receive(&[IAC, SB, OPT_TN3270E, E_DEVICE_TYPE, E_REJECT, E_REASON, 4, IAC, SE]);
        t.take_output();
        t.receive(&[IAC, DO, OPT_TTYPE, IAC, SB, OPT_TTYPE, TTYPE_SEND, IAC, SE]);
        let out = t.take_output();
        assert!(out.windows(TT.len()).any(|w| w == TT.as_bytes()));
        let ev = t.receive(&[IAC, DO, OPT_EOR, IAC, WILL, OPT_EOR, IAC, DO, OPT_BINARY, IAC, WILL, OPT_BINARY]);
        assert_eq!(ev, vec![Event::Ready { tn3270e: false, device_type: TT.into(), lu: None }]);
        let ev = t.receive(&[0xF5, 0xC3, IAC, EOR]);
        assert_eq!(ev, vec![Event::Data(vec![0xF5, 0xC3])]);
        assert_eq!(t.encode_record(0, &[0x7D]), vec![0x7D, IAC, EOR]);
    }

    #[test]
    fn refuses_unknown_options_once() {
        let mut t = Telnet::new(TT, None);
        t.receive(&[IAC, DO, 1, IAC, WILL, 3]);
        assert_eq!(t.take_output(), vec![IAC, WONT, 1, IAC, DONT, 3]);
        t.receive(&[IAC, DO, OPT_EOR, IAC, DO, OPT_EOR]);
        assert_eq!(t.take_output(), vec![IAC, WILL, OPT_EOR]);
    }
}
