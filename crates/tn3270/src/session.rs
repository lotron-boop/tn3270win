//! A TN3270 session: Telnet plus screen, with no I/O of its own.

use crate::codes::*;
use crate::ebcdic::CodePage;
use crate::query::query_reply;
use crate::screen::{Aid, InputError, Model, Screen};
use crate::telnet::{Event, Telnet, DT_3270_DATA, DT_SSCP_LU_DATA};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionState {
    /// TCP connected, Telnet negotiation in progress.
    Negotiating,
    /// 3270 mode.
    Ready { tn3270e: bool, device_type: String, lu: Option<String> },
}

pub struct Session {
    telnet: Telnet,
    pub screen: Screen,
    pub state: SessionState,
    out: Vec<u8>,
    /// Set whenever the screen may need repainting.
    pub dirty: bool,
}

impl Session {
    pub fn new(codepage: CodePage, lu: Option<String>) -> Session {
        Session::with_model(Model::M2, codepage, lu)
    }

    pub fn with_model(model: Model, codepage: CodePage, lu: Option<String>) -> Session {
        Session {
            telnet: Telnet::new(&model.terminal_type(), lu),
            screen: Screen::for_model(model, codepage),
            state: SessionState::Negotiating,
            out: Vec::new(),
            dirty: true,
        }
    }

    /// Bytes to write to the socket.
    pub fn take_output(&mut self) -> Vec<u8> {
        let mut v = self.telnet.take_output();
        v.append(&mut self.out);
        v
    }

    /// Feed bytes read from the socket.
    pub fn receive(&mut self, bytes: &[u8]) {
        for ev in self.telnet.receive(bytes) {
            match ev {
                Event::Ready { tn3270e, device_type, lu } => {
                    self.state = SessionState::Ready { tn3270e, device_type, lu };
                }
                Event::Data(rec) => self.record(&rec),
                Event::SscpLu(data) => self.screen.sscp_write(&data),
                Event::Unbind => {
                    self.screen.clear();
                    self.screen.reset();
                }
            }
            self.dirty = true;
        }
    }

    fn send_record(&mut self, data_type: u8, data: &[u8]) {
        let rec = self.telnet.encode_record(data_type, data);
        self.out.extend_from_slice(&rec);
    }

    fn record(&mut self, rec: &[u8]) {
        let Some(&cmd) = rec.first() else { return };
        let data = &rec[1..];
        match cmd {
            c if CMD_W.contains(&c) => self.screen.write(data, false),
            c if CMD_EW.contains(&c) || CMD_EWA.contains(&c) => {
                self.screen.set_alternate(CMD_EWA.contains(&c));
                self.screen.write(data, true);
            }
            c if CMD_EAU.contains(&c) => self.screen.erase_all_unprotected(),
            c if CMD_RB.contains(&c) => {
                let r = self.screen.read_buffer();
                self.send_record(DT_3270_DATA, &r);
            }
            c if CMD_RM.contains(&c) || CMD_RMA.contains(&c) => {
                let r = self.screen.host_read_modified(CMD_RMA.contains(&c));
                self.send_record(DT_3270_DATA, &r);
            }
            c if CMD_WSF.contains(&c) => self.structured_fields(data),
            _ => {} // NOP and unknown commands
        }
    }

    fn structured_fields(&mut self, mut data: &[u8]) {
        while data.len() >= 3 {
            let mut len = u16::from_be_bytes([data[0], data[1]]) as usize;
            if len == 0 {
                len = data.len(); // "rest of the data"
            }
            if len < 3 || len > data.len() {
                break;
            }
            let (sf, rest) = data.split_at(len);
            data = rest;
            match sf[2] {
                // Read Partition: Query or Query List.
                0x01 if sf.len() >= 5 && sf[3] == 0xFF && matches!(sf[4], 0x02 | 0x03) => {
                    let (r, c) = self.screen.alternate_size();
                    let q = query_reply(r as u16, c as u16);
                    self.send_record(DT_3270_DATA, &q);
                }
                // Erase/Reset: flag 0x80 selects the alternate size.
                0x03 => {
                    let alt = sf.get(3).is_some_and(|f| f & 0x80 != 0);
                    self.screen.set_alternate(alt);
                    self.screen.clear();
                }
                // Outbound 3270DS: partition, command, then write data.
                0x40 if sf.len() >= 5 => {
                    let cmd = sf[4];
                    let body = &sf[5..];
                    if CMD_W.contains(&cmd) {
                        self.screen.write(body, false);
                    } else if CMD_EW.contains(&cmd) || CMD_EWA.contains(&cmd) {
                        self.screen.set_alternate(CMD_EWA.contains(&cmd));
                        self.screen.write(body, true);
                    } else if CMD_EAU.contains(&cmd) {
                        self.screen.erase_all_unprotected();
                    }
                }
                _ => {}
            }
        }
    }

    /// Operator pressed an AID key (Enter, Clear, PFn, PAn).
    pub fn aid(&mut self, aid: Aid) -> Result<(), InputError> {
        if !matches!(self.state, SessionState::Ready { .. }) {
            return Err(InputError::Locked);
        }
        self.dirty = true;
        if self.screen.in_sscp_mode() && aid == Aid::Enter {
            if self.screen.keyboard == crate::screen::Lock::Wait {
                return Err(InputError::Locked);
            }
            let text = self.screen.sscp_input();
            self.send_record(DT_SSCP_LU_DATA, &text);
            return Ok(());
        }
        let data = self.screen.press_aid(aid)?;
        self.send_record(DT_3270_DATA, &data);
        Ok(())
    }

    /// ATTN: Telnet Interrupt Process.
    pub fn attn(&mut self) {
        let ip = self.telnet.interrupt();
        self.out.extend_from_slice(&ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telnet::*;

    fn ready_classic() -> Session {
        let mut s = Session::new(CodePage::Cp037, None);
        s.receive(&[IAC, DO, OPT_EOR, IAC, WILL, OPT_EOR, IAC, DO, OPT_BINARY, IAC, WILL, OPT_BINARY]);
        s.take_output();
        s
    }

    #[test]
    fn answers_query_and_shows_screen() {
        let mut s = ready_classic();
        assert!(matches!(s.state, SessionState::Ready { tn3270e: false, .. }));
        // WSF Read Partition Query
        s.receive(&[0xF3, 0x00, 0x05, 0x01, 0xFF, 0xFF, 0x02, IAC, EOR]); // 0xFF doubled on the wire
        let out = s.take_output();
        assert_eq!(out[0], 0x88);
        assert_eq!(&out[out.len() - 2..], &[IAC, EOR]);
        // Erase/Write with restore, IC, then text.
        let mut rec = vec![0xF5, WCC_KEYBOARD_RESTORE, ORDER_IC];
        rec.extend(CodePage::Cp037.encode("WELCOME"));
        rec.extend_from_slice(&[IAC, EOR]);
        s.receive(&rec);
        assert!(s.screen.row_text(0).starts_with("WELCOME"));
        s.screen.type_char('X').unwrap();
        s.aid(Aid::Enter).unwrap();
        let out = s.take_output();
        assert_eq!(out[0], 0x7D);
        assert_eq!(s.aid(Aid::Enter), Err(InputError::Locked));
    }

    #[test]
    fn host_read_buffer() {
        let mut s = ready_classic();
        s.receive(&[0xF5, WCC_KEYBOARD_RESTORE, IAC, EOR, 0xF2, IAC, EOR]);
        let out = s.take_output();
        assert_eq!(out.len(), 1 + 2 + 1920 + 2);
        assert_eq!(out[0], 0x60);
    }

    #[test]
    fn model_4_terminal_type_and_query() {
        let mut s = Session::with_model(Model::M4, CodePage::Cp037, None);
        s.receive(&[IAC, DO, OPT_TTYPE, IAC, SB, OPT_TTYPE, 1, IAC, SE]);
        let out = s.take_output();
        assert!(out.windows(12).any(|w| w == b"IBM-3278-4-E"));
        s.receive(&[IAC, DO, OPT_EOR, IAC, WILL, OPT_EOR, IAC, DO, OPT_BINARY, IAC, WILL, OPT_BINARY]);
        s.take_output();
        s.receive(&[0x7E, WCC_KEYBOARD_RESTORE, IAC, EOR]); // Erase/Write Alternate
        assert_eq!((s.screen.rows, s.screen.cols), (43, 80));
    }

    #[test]
    fn not_ready_rejects_keys() {
        let mut s = Session::new(CodePage::Cp037, None);
        assert_eq!(s.aid(Aid::Enter), Err(InputError::Locked));
    }
}
