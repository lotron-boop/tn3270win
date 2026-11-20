//! The 3270 presentation space: host writes, fields, local editing and
//! inbound (terminal-to-host) data.

use crate::codes::*;
use crate::ebcdic::CodePage;

/// One buffer position. A field-attribute position (`fa` is `Some`) shows
/// as a blank and starts a field; `color`/`hilite` there apply to the whole
/// field. On a character position they come from Set Attribute (0 = inherit).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub byte: u8,
    pub fa: Option<u8>,
    pub color: u8,
    pub hilite: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    /// Waiting for the host (X SYSTEM / X clock).
    Locked,
    /// Cursor is in a protected field.
    Protected,
    /// Non-numeric character in a numeric field.
    Numeric,
    /// Insert mode with no room left in the field.
    Overflow,
    /// The character isn't in the host code page.
    BadChar,
}

impl InputError {
    /// Status-line text, in the style of the 3270 operator information area.
    pub fn oia_text(self) -> &'static str {
        match self {
            InputError::Locked => "X SYSTEM",
            InputError::Protected => "X Protected",
            InputError::Numeric => "X NUM",
            InputError::Overflow => "X Overflow",
            InputError::BadChar => "X Invalid character",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lock {
    Unlocked,
    /// An AID was sent; waiting for the host to restore the keyboard.
    Wait,
    /// An operator error; cleared by Reset.
    Error(InputError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aid {
    NoAid,
    Enter,
    Clear,
    Pf(u8),
    Pa(u8),
    SysReq,
    StructuredField,
}

impl Aid {
    pub fn code(self) -> u8 {
        const PF: [u8; 24] = [
            0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0x7A, 0x7B, 0x7C,
            0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0x4A, 0x4B, 0x4C,
        ];
        match self {
            Aid::NoAid => 0x60,
            Aid::Enter => 0x7D,
            Aid::Clear => 0x6D,
            Aid::Pf(n) => PF[(n.clamp(1, 24) - 1) as usize],
            Aid::Pa(1) => 0x6C,
            Aid::Pa(2) => 0x6E,
            Aid::Pa(_) => 0x6B,
            Aid::SysReq => 0xF0,
            Aid::StructuredField => 0x88,
        }
    }

    /// Clear and PA keys send only the AID ("short read").
    pub fn is_short_read(self) -> bool {
        matches!(self, Aid::Clear | Aid::Pa(_))
    }
}

/// The 3270 colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Blue,
    Red,
    Pink,
    Green,
    Turquoise,
    Yellow,
    White,
    Black,
    DeepBlue,
    Orange,
    Purple,
    PaleGreen,
    PaleTurquoise,
    Grey,
}

impl Color {
    fn from_code(c: u8) -> Option<Color> {
        Some(match c {
            0xF1 => Color::Blue,
            0xF2 => Color::Red,
            0xF3 => Color::Pink,
            0xF4 => Color::Green,
            0xF5 => Color::Turquoise,
            0xF6 => Color::Yellow,
            0xF7 | 0xFF => Color::White,
            0xF8 => Color::Black,
            0xF9 => Color::DeepBlue,
            0xFA => Color::Orange,
            0xFB => Color::Purple,
            0xFC => Color::PaleGreen,
            0xFD => Color::PaleTurquoise,
            0xFE => Color::Grey,
            _ => return None,
        })
    }
}

/// What to draw at one position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub color: Color,
    pub reverse: bool,
    pub underline: bool,
}

pub struct Screen {
    pub rows: usize,
    pub cols: usize,
    cells: Vec<Cell>,
    pub cursor: usize,
    pub keyboard: Lock,
    pub insert: bool,
    /// Set by the host's "sound alarm"; the UI beeps and clears it.
    pub alarm: bool,
    pub codepage: CodePage,
    aid: Aid,
    /// In SSCP-LU mode, where the operator's input starts.
    sscp_start: Option<usize>,
}

impl Screen {
    pub fn new(rows: usize, cols: usize, codepage: CodePage) -> Screen {
        Screen {
            rows,
            cols,
            cells: vec![Cell::default(); rows * cols],
            cursor: 0,
            keyboard: Lock::Unlocked,
            insert: false,
            alarm: false,
            codepage,
            aid: Aid::NoAid,
            sscp_start: None,
        }
    }

    pub fn size(&self) -> usize {
        self.cells.len()
    }

    pub fn cell(&self, pos: usize) -> &Cell {
        &self.cells[pos % self.size()]
    }

    fn inc(&self, p: usize) -> usize {
        (p + 1) % self.size()
    }

    fn dec(&self, p: usize) -> usize {
        (p + self.size() - 1) % self.size()
    }

    pub fn clear(&mut self) {
        self.cells.iter_mut().for_each(|c| *c = Cell::default());
        self.cursor = 0;
    }

    /// The text of one row, as displayed (hidden fields blank).
    pub fn row_text(&self, row: usize) -> String {
        (row * self.cols..(row + 1) * self.cols).map(|p| self.glyph(p).ch).collect()
    }

    pub fn text(&self) -> String {
        let g = self.glyphs();
        (0..self.rows)
            .map(|r| g[r * self.cols..(r + 1) * self.cols].iter().map(|x| x.ch).collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    // ---- fields -----------------------------------------------------------

    pub fn is_formatted(&self) -> bool {
        self.cells.iter().any(|c| c.fa.is_some())
    }

    fn is_fa(&self, p: usize) -> bool {
        self.cells[p].fa.is_some()
    }

    /// Position of the attribute that governs `p` (itself if it is one).
    fn field_start(&self, p: usize) -> Option<usize> {
        let mut q = p;
        for _ in 0..self.size() {
            if self.is_fa(q) {
                return Some(q);
            }
            q = self.dec(q);
        }
        None
    }

    fn attr_of(&self, p: usize) -> Option<u8> {
        self.field_start(p).and_then(|f| self.cells[f].fa)
    }

    fn is_protected(&self, p: usize) -> bool {
        self.is_fa(p) || self.attr_of(p).is_some_and(|a| a & FA_PROTECTED != 0)
    }

    /// Data positions from `p` to the end of its field (or of the buffer
    /// when unformatted).
    fn rest_of_field(&self, p: usize) -> Vec<usize> {
        if !self.is_formatted() {
            return (p..self.size()).collect();
        }
        let mut v = Vec::new();
        let mut q = p;
        for _ in 0..self.size() {
            if self.is_fa(q) {
                break;
            }
            v.push(q);
            q = self.inc(q);
        }
        v
    }

    fn set_mdt(&mut self, p: usize) {
        if let Some(f) = self.field_start(p) {
            if let Some(a) = self.cells[f].fa.as_mut() {
                *a |= FA_MDT;
            }
        }
    }

    fn reset_all_mdt(&mut self) {
        for c in &mut self.cells {
            if let Some(a) = c.fa.as_mut() {
                *a &= !FA_MDT;
            }
        }
    }

    /// First data position of the next unprotected field, scanning from
    /// `from` (an attribute at `from` itself counts).
    fn tab_target(&self, from: usize) -> Option<usize> {
        let mut p = from % self.size();
        for _ in 0..self.size() {
            if let Some(a) = self.cells[p].fa {
                let next = self.inc(p);
                if a & FA_PROTECTED == 0 && !self.is_fa(next) {
                    return Some(next);
                }
            }
            p = self.inc(p);
        }
        None
    }

    // ---- host writes ------------------------------------------------------

    /// Process a Write, Erase/Write or Erase/Write Alternate. `data` starts
    /// with the WCC.
    pub fn write(&mut self, data: &[u8], erase: bool) {
        self.sscp_start = None;
        if erase {
            self.clear();
        }
        let Some(&wcc) = data.first() else { return };
        if wcc & WCC_RESET_MDT != 0 {
            self.reset_all_mdt();
        }
        let size = self.size();
        let mut addr = self.cursor;
        let (mut color, mut hilite) = (0u8, 0u8);
        let mut last_was_data = false;
        let mut i = 1;
        let at = |i: usize| data.get(i).copied();

        while i < data.len() {
            let b = data[i];
            match b {
                ORDER_SF => {
                    let Some(attr) = at(i + 1) else { break };
                    self.cells[addr] = Cell { byte: 0, fa: Some(attr & 0x3F), color: 0, hilite: 0 };
                    addr = self.inc(addr);
                    i += 2;
                    last_was_data = false;
                }
                ORDER_SFE | ORDER_MF => {
                    let Some(n) = at(i + 1) else { break };
                    let n = n as usize;
                    if i + 2 + 2 * n > data.len() {
                        break;
                    }
                    let mut cell = if b == ORDER_MF { self.cells[addr] } else {
                        Cell { byte: 0, fa: Some(0), color: 0, hilite: 0 }
                    };
                    for k in 0..n {
                        let (t, v) = (data[i + 2 + 2 * k], data[i + 3 + 2 * k]);
                        match t {
                            XA_3270 => cell.fa = Some(v & 0x3F),
                            XA_FOREGROUND => cell.color = v,
                            XA_HIGHLIGHT => cell.hilite = v,
                            XA_ALL if b == ORDER_MF => { cell.color = 0; cell.hilite = 0; }
                            _ => {}
                        }
                    }
                    if b == ORDER_SFE || self.is_fa(addr) {
                        self.cells[addr] = cell;
                    }
                    addr = self.inc(addr);
                    i += 2 + 2 * n;
                    last_was_data = false;
                }
                ORDER_SBA => {
                    let (Some(b1), Some(b2)) = (at(i + 1), at(i + 2)) else { break };
                    addr = decode_address(b1, b2) % size;
                    i += 3;
                    last_was_data = false;
                }
                ORDER_SA => {
                    let (Some(t), Some(v)) = (at(i + 1), at(i + 2)) else { break };
                    match t {
                        XA_ALL => { color = 0; hilite = 0; }
                        XA_FOREGROUND => color = v,
                        XA_HIGHLIGHT => hilite = v,
                        _ => {}
                    }
                    i += 3;
                }
                ORDER_IC => {
                    self.cursor = addr;
                    i += 1;
                }
                ORDER_PT => {
                    if last_was_data {
                        // PT right after data nulls the rest of that field.
                        while !self.is_fa(addr) {
                            self.cells[addr].byte = EBC_NULL;
                            addr = self.inc(addr);
                            if addr == 0 { break; }
                        }
                    }
                    addr = self.tab_target(addr).unwrap_or(0);
                    i += 1;
                    last_was_data = false;
                }
                ORDER_RA => {
                    let (Some(b1), Some(b2), Some(mut ch)) = (at(i + 1), at(i + 2), at(i + 3)) else { break };
                    i += 4;
                    if ch == ORDER_GE {
                        let Some(g) = at(i) else { break };
                        ch = g;
                        i += 1;
                    }
                    let stop = decode_address(b1, b2) % size;
                    loop {
                        self.cells[addr] = Cell { byte: ch, fa: None, color, hilite };
                        addr = self.inc(addr);
                        if addr == stop { break; }
                    }
                    last_was_data = true;
                }
                ORDER_EUA => {
                    let (Some(b1), Some(b2)) = (at(i + 1), at(i + 2)) else { break };
                    let stop = decode_address(b1, b2) % size;
                    loop {
                        if !self.is_protected(addr) {
                            self.cells[addr] = Cell::default();
                        }
                        addr = self.inc(addr);
                        if addr == stop { break; }
                    }
                    i += 3;
                    last_was_data = false;
                }
                ORDER_GE => {
                    let Some(ch) = at(i + 1) else { break };
                    self.cells[addr] = Cell { byte: ch, fa: None, color, hilite };
                    addr = self.inc(addr);
                    i += 2;
                    last_was_data = true;
                }
                _ => {
                    self.cells[addr] = Cell { byte: b, fa: None, color, hilite };
                    addr = self.inc(addr);
                    i += 1;
                    last_was_data = true;
                }
            }
        }

        if wcc & WCC_KEYBOARD_RESTORE != 0 {
            self.keyboard = Lock::Unlocked;
            self.aid = Aid::NoAid;
        }
        if wcc & WCC_ALARM != 0 {
            self.alarm = true;
        }
    }

    /// Erase All Unprotected.
    pub fn erase_all_unprotected(&mut self) {
        self.sscp_start = None;
        for p in 0..self.size() {
            if !self.is_protected(p) {
                self.cells[p] = Cell::default();
            }
        }
        for c in &mut self.cells {
            if let Some(a) = c.fa.as_mut() {
                if *a & FA_PROTECTED == 0 {
                    *a &= !FA_MDT;
                }
            }
        }
        self.cursor = self.tab_target(0).unwrap_or(0);
        self.keyboard = Lock::Unlocked;
        self.aid = Aid::NoAid;
    }

    /// TN3270E SSCP-LU data: unformatted text from the network.
    pub fn sscp_write(&mut self, data: &[u8]) {
        if self.sscp_start.is_none() {
            self.clear();
        }
        for &b in data {
            match b {
                0x15 | 0x25 => {
                    let row = (self.cursor / self.cols + 1) % self.rows;
                    self.cursor = row * self.cols;
                }
                0x00..=0x3F => {}
                _ => {
                    self.cells[self.cursor] = Cell { byte: b, ..Cell::default() };
                    self.cursor = self.inc(self.cursor);
                }
            }
        }
        self.sscp_start = Some(self.cursor);
        self.keyboard = Lock::Unlocked;
    }

    pub fn in_sscp_mode(&self) -> bool {
        self.sscp_start.is_some()
    }

    /// What the operator typed since the last SSCP-LU write.
    pub fn sscp_input(&mut self) -> Vec<u8> {
        let Some(start) = self.sscp_start else { return Vec::new() };
        let mut v = Vec::new();
        let mut p = start;
        while p != self.cursor {
            if self.cells[p].byte != EBC_NULL {
                v.push(self.cells[p].byte);
            }
            p = self.inc(p);
        }
        self.keyboard = Lock::Wait;
        v
    }

    // ---- inbound ----------------------------------------------------------

    /// Read Buffer response.
    pub fn read_buffer(&self) -> Vec<u8> {
        let mut v = vec![self.aid.code()];
        v.extend_from_slice(&encode_address(self.cursor));
        for c in &self.cells {
            match c.fa {
                Some(a) => v.extend_from_slice(&[ORDER_SF, CODE_TABLE[(a & 0x3F) as usize]]),
                None => v.push(c.byte),
            }
        }
        v
    }

    /// Read Modified response for `aid`. `all` = Read Modified All (no
    /// short read).
    pub fn read_modified(&self, aid: Aid, all: bool) -> Vec<u8> {
        let mut v = vec![aid.code()];
        if aid.is_short_read() && !all {
            return v;
        }
        v.extend_from_slice(&encode_address(self.cursor));
        if !self.is_formatted() {
            v.extend(self.cells.iter().map(|c| c.byte).filter(|&b| b != EBC_NULL));
            return v;
        }
        for p in 0..self.size() {
            if let Some(a) = self.cells[p].fa {
                if a & FA_MDT != 0 {
                    let start = self.inc(p);
                    v.push(ORDER_SBA);
                    v.extend_from_slice(&encode_address(start));
                    for q in self.rest_of_field(start) {
                        let b = self.cells[q].byte;
                        if b != EBC_NULL {
                            v.push(b);
                        }
                    }
                }
            }
        }
        v
    }

    /// The host's Read Modified: answered with the pending AID.
    pub fn host_read_modified(&self, all: bool) -> Vec<u8> {
        self.read_modified(self.aid, all)
    }

    /// The operator pressed an AID key: returns the inbound data to send.
    pub fn press_aid(&mut self, aid: Aid) -> Result<Vec<u8>, InputError> {
        if self.keyboard == Lock::Wait {
            return Err(InputError::Locked);
        }
        if aid == Aid::Clear {
            self.clear();
            self.sscp_start = None;
        }
        self.aid = aid;
        self.insert = false;
        self.keyboard = Lock::Wait;
        Ok(self.read_modified(aid, false))
    }

    // ---- operator input ---------------------------------------------------

    fn ready(&self) -> Result<(), InputError> {
        match self.keyboard {
            Lock::Unlocked => Ok(()),
            Lock::Wait => Err(InputError::Locked),
            Lock::Error(e) => Err(e),
        }
    }

    /// Record an operator error so the status line shows it until Reset.
    fn fail(&mut self, e: InputError) -> Result<(), InputError> {
        if e != InputError::Locked {
            self.keyboard = Lock::Error(e);
        }
        Err(e)
    }

    pub fn reset(&mut self) {
        self.keyboard = Lock::Unlocked;
        self.insert = false;
    }

    pub fn toggle_insert(&mut self) {
        self.insert = !self.insert;
    }

    /// Type a character at the cursor.
    pub fn type_char(&mut self, c: char) -> Result<(), InputError> {
        match self.codepage.from_char(c) {
            Some(b) => self.type_byte(b),
            None => match self.ready() {
                Ok(()) => self.fail(InputError::BadChar),
                Err(e) => Err(e),
            },
        }
    }

    pub fn type_byte(&mut self, b: u8) -> Result<(), InputError> {
        self.ready()?;
        let pos = self.cursor;
        if self.is_formatted() {
            if self.is_protected(pos) {
                return self.fail(InputError::Protected);
            }
            let numeric = self.attr_of(pos).is_some_and(|a| a & FA_NUMERIC != 0);
            let ok_numeric = matches!(b, 0xF0..=0xF9 | 0x4B | 0x60 | EBC_DUP | EBC_FM);
            if numeric && !ok_numeric {
                return self.fail(InputError::Numeric);
            }
        }
        let rest = self.rest_of_field(pos);
        if self.insert {
            let last = *rest.last().unwrap_or(&pos);
            if !matches!(self.cells[last].byte, EBC_NULL | EBC_SPACE) {
                return self.fail(InputError::Overflow);
            }
            for k in (1..rest.len()).rev() {
                self.cells[rest[k]].byte = self.cells[rest[k - 1]].byte;
            }
        }
        self.cells[pos].byte = b;
        self.set_mdt(pos);
        let next = self.inc(pos);
        self.cursor = if self.is_fa(next) { self.tab_target(next).unwrap_or(next) } else { next };
        Ok(())
    }

    pub fn tab(&mut self) {
        self.cursor = self.tab_target(self.inc(self.cursor)).unwrap_or(0);
    }

    pub fn back_tab(&mut self) {
        if !self.is_formatted() {
            self.cursor = 0;
            return;
        }
        let pos = self.cursor;
        if !self.is_protected(pos) {
            if let Some(f) = self.field_start(pos) {
                let start = self.inc(f);
                if start != pos {
                    self.cursor = start;
                    return;
                }
            }
        }
        let mut p = self.dec(pos);
        for _ in 0..self.size() {
            if let Some(a) = self.cells[p].fa {
                let s = self.inc(p);
                if a & FA_PROTECTED == 0 && !self.is_fa(s) && s != pos {
                    self.cursor = s;
                    return;
                }
            }
            p = self.dec(p);
        }
    }

    pub fn home(&mut self) {
        self.cursor = if self.is_formatted() { self.tab_target(0).unwrap_or(0) } else { 0 };
    }

    pub fn new_line(&mut self) {
        let row = (self.cursor / self.cols + 1) % self.rows;
        let start = row * self.cols;
        if !self.is_formatted() {
            self.cursor = start;
            return;
        }
        let mut p = start;
        for _ in 0..self.size() {
            if !self.is_protected(p) {
                self.cursor = p;
                return;
            }
            p = self.inc(p);
        }
    }

    pub fn end_of_field(&mut self) {
        if self.is_protected(self.cursor) {
            return;
        }
        let rest = self.rest_of_field(self.cursor);
        // After the last non-blank character in the field.
        let last = rest.iter().rposition(|&q| !matches!(self.cells[q].byte, EBC_NULL | EBC_SPACE));
        self.cursor = match last {
            Some(i) if i + 1 < rest.len() => rest[i + 1],
            Some(i) => rest[i],
            None => self.cursor,
        };
    }

    pub fn move_cursor(&mut self, drow: isize, dcol: isize) {
        let size = self.size() as isize;
        let delta = drow * self.cols as isize + dcol;
        self.cursor = (self.cursor as isize + delta).rem_euclid(size) as usize;
    }

    fn is_blank(&self, p: usize) -> bool {
        self.is_fa(p) || matches!(self.cells[p].byte, EBC_NULL | EBC_SPACE)
    }

    pub fn word_right(&mut self) {
        let mut p = self.cursor;
        let mut n = 0;
        while !self.is_blank(p) && n < self.size() { p = self.inc(p); n += 1; }
        while self.is_blank(p) && n < self.size() { p = self.inc(p); n += 1; }
        self.cursor = p;
    }

    pub fn word_left(&mut self) {
        let mut p = self.dec(self.cursor);
        let mut n = 0;
        while self.is_blank(p) && n < self.size() { p = self.dec(p); n += 1; }
        while !self.is_blank(self.dec(p)) && n < self.size() { p = self.dec(p); n += 1; }
        self.cursor = p;
    }

    /// Delete the character at the cursor, closing up the field.
    pub fn delete(&mut self) -> Result<(), InputError> {
        self.ready()?;
        if self.is_formatted() && self.is_protected(self.cursor) {
            return self.fail(InputError::Protected);
        }
        let rest = self.rest_of_field(self.cursor);
        for k in 0..rest.len().saturating_sub(1) {
            self.cells[rest[k]].byte = self.cells[rest[k + 1]].byte;
        }
        if let Some(&last) = rest.last() {
            self.cells[last].byte = EBC_NULL;
        }
        self.set_mdt(self.cursor);
        Ok(())
    }

    /// Move left and delete (does nothing at the start of a field).
    pub fn backspace(&mut self) -> Result<(), InputError> {
        self.ready()?;
        let pos = self.cursor;
        if self.is_formatted() {
            if self.is_protected(pos) {
                return self.fail(InputError::Protected);
            }
            if self.field_start(pos).map(|f| self.inc(f)) == Some(pos) {
                return Ok(());
            }
        } else if pos == 0 {
            return Ok(());
        }
        self.cursor = self.dec(pos);
        self.delete()
    }

    pub fn erase_eof(&mut self) -> Result<(), InputError> {
        self.ready()?;
        if self.is_formatted() && self.is_protected(self.cursor) {
            return self.fail(InputError::Protected);
        }
        for q in self.rest_of_field(self.cursor) {
            self.cells[q].byte = EBC_NULL;
        }
        self.set_mdt(self.cursor);
        Ok(())
    }

    pub fn erase_input(&mut self) -> Result<(), InputError> {
        self.ready()?;
        if !self.is_formatted() {
            self.clear();
            return Ok(());
        }
        for p in 0..self.size() {
            if !self.is_protected(p) {
                self.cells[p].byte = EBC_NULL;
            }
        }
        for c in &mut self.cells {
            if let Some(a) = c.fa.as_mut() {
                if *a & FA_PROTECTED == 0 {
                    *a &= !FA_MDT;
                }
            }
        }
        self.home();
        Ok(())
    }

    pub fn dup(&mut self) -> Result<(), InputError> {
        self.type_byte(EBC_DUP)?;
        if !self.is_fa(self.dec(self.cursor)) {
            self.tab();
        }
        Ok(())
    }

    pub fn field_mark(&mut self) -> Result<(), InputError> {
        self.type_byte(EBC_FM)
    }

    // ---- display ------------------------------------------------------------

    pub fn glyph(&self, pos: usize) -> Glyph {
        self.glyph_in_field(pos, self.field_start(pos))
    }

    /// Every position's glyph in one pass (what the UI paints).
    pub fn glyphs(&self) -> Vec<Glyph> {
        // The field covering position 0 starts at the last attribute in the
        // buffer (fields wrap around).
        let mut field = self.cells.iter().rposition(|c| c.fa.is_some());
        (0..self.size())
            .map(|p| {
                if self.cells[p].fa.is_some() {
                    field = Some(p);
                }
                self.glyph_in_field(p, field)
            })
            .collect()
    }

    fn glyph_in_field(&self, pos: usize, field: Option<usize>) -> Glyph {
        let cell = self.cells[pos];
        let (fa, fcolor, fhilite) = match field {
            Some(f) => (self.cells[f].fa, self.cells[f].color, self.cells[f].hilite),
            None => (None, 0, 0),
        };
        let attr = fa.unwrap_or(0);
        let color_code = if cell.fa.is_none() && cell.color != 0 { cell.color } else { fcolor };
        let hilite = if cell.fa.is_none() && cell.hilite != 0 { cell.hilite } else { fhilite };
        let color = Color::from_code(color_code).unwrap_or(match fa {
            None => Color::Green,
            Some(_) => {
                let bright = attr & FA_INTENSITY == FA_INT_HIGH;
                match (attr & FA_PROTECTED != 0, bright) {
                    (true, true) => Color::White,
                    (true, false) => Color::Blue,
                    (false, true) => Color::Red,
                    (false, false) => Color::Green,
                }
            }
        });
        let hidden = attr & FA_INTENSITY == FA_INT_ZERO;
        let ch = if cell.fa.is_some() || hidden {
            ' '
        } else {
            match cell.byte {
                EBC_DUP => '*',
                EBC_FM => ';',
                b => self.codepage.to_char(b),
            }
        };
        Glyph {
            ch,
            color,
            reverse: cell.fa.is_none() && hilite == 0xF2,
            underline: cell.fa.is_none() && hilite == 0xF4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp() -> CodePage {
        CodePage::Cp037
    }

    fn addr(row: usize, col: usize) -> [u8; 2] {
        encode_address(row * 80 + col)
    }

    /// A logon-style screen: protected label, unprotected input (row 0,
    /// col 10-19), protected numeric (skip) stopper, second input on row 1.
    fn logon() -> Screen {
        let mut s = Screen::new(24, 80, cp());
        let mut d = vec![WCC_KEYBOARD_RESTORE | WCC_RESET_MDT];
        d.extend_from_slice(&[ORDER_SF, FA_PROTECTED]);
        d.extend(cp().encode("USERID"));
        d.push(ORDER_SBA); d.extend_from_slice(&addr(0, 9));
        d.extend_from_slice(&[ORDER_SF, 0x00, ORDER_IC]);
        d.push(ORDER_SBA); d.extend_from_slice(&addr(0, 20));
        d.extend_from_slice(&[ORDER_SF, FA_PROTECTED | FA_NUMERIC]);
        d.push(ORDER_SBA); d.extend_from_slice(&addr(1, 9));
        d.extend_from_slice(&[ORDER_SF, FA_INT_ZERO]); // hidden password field
        d.push(ORDER_SBA); d.extend_from_slice(&addr(1, 20));
        d.extend_from_slice(&[ORDER_SF, FA_PROTECTED | FA_NUMERIC]);
        s.write(&d, true);
        s
    }

    #[test]
    fn erase_write_builds_fields() {
        let s = logon();
        assert!(s.row_text(0).starts_with(" USERID"));
        assert_eq!(s.cursor, 10);
        assert_eq!(s.keyboard, Lock::Unlocked);
        assert_eq!(s.glyph(1).color, Color::Blue);
        assert_eq!(s.glyph(10).color, Color::Green);
    }

    #[test]
    fn typing_autoskips_and_read_modified() {
        let mut s = logon();
        for c in "ABCDEFGHIJ".chars() {
            s.type_char(c).unwrap();
        }
        // Field full: cursor skipped the protected stopper to the next input.
        assert_eq!(s.cursor, 90);
        s.type_char('x').unwrap();
        assert_eq!(s.row_text(1).trim(), "", "password field is not displayed");
        let rm = s.press_aid(Aid::Enter).unwrap();
        let mut want = vec![0x7D];
        want.extend_from_slice(&encode_address(91));
        want.push(ORDER_SBA); want.extend_from_slice(&addr(0, 10));
        want.extend(cp().encode("ABCDEFGHIJ"));
        want.push(ORDER_SBA); want.extend_from_slice(&addr(1, 10));
        want.extend(cp().encode("x"));
        assert_eq!(rm, want);
        assert_eq!(s.keyboard, Lock::Wait);
        assert_eq!(s.type_char('y'), Err(InputError::Locked));
    }

    #[test]
    fn protected_and_numeric_errors_need_reset() {
        let mut s = logon();
        s.cursor = 2;
        assert_eq!(s.type_char('Z'), Err(InputError::Protected));
        s.cursor = 10;
        assert_eq!(s.type_char('A'), Err(InputError::Protected), "still locked until Reset");
        s.reset();
        s.type_char('A').unwrap();
    }

    #[test]
    fn insert_mode_and_overflow() {
        let mut s = logon();
        for c in "ACD".chars() { s.type_char(c).unwrap(); }
        s.cursor = 11;
        s.toggle_insert();
        s.type_char('B').unwrap();
        assert_eq!(&s.row_text(0)[10..14], "ABCD");
        for c in "EFGHIJ".chars() { s.type_char(c).unwrap(); }
        assert_eq!(s.type_char('K'), Err(InputError::Overflow));
    }

    #[test]
    fn editing_keys() {
        let mut s = logon();
        for c in "HELLO".chars() { s.type_char(c).unwrap(); }
        s.cursor = 11;
        s.delete().unwrap();
        assert_eq!(&s.row_text(0)[10..15], "HLLO ");
        s.backspace().unwrap();
        assert_eq!(s.cursor, 10);
        assert_eq!(&s.row_text(0)[10..14], "LLO ");
        s.backspace().unwrap(); // at field start: no-op
        assert_eq!(s.cursor, 10);
        s.cursor = 11;
        s.erase_eof().unwrap();
        assert_eq!(&s.row_text(0)[10..13], "L  ");
        s.cursor = 10;
        s.end_of_field();
        assert_eq!(s.cursor, 11);
    }

    #[test]
    fn tab_backtab_home_newline() {
        let mut s = logon();
        s.tab();
        assert_eq!(s.cursor, 90);
        s.tab();
        assert_eq!(s.cursor, 10, "wraps to the first input field");
        s.back_tab();
        assert_eq!(s.cursor, 90);
        s.cursor = 95;
        s.back_tab();
        assert_eq!(s.cursor, 90, "first back tab goes to the start of the field");
        s.cursor = 300;
        s.home();
        assert_eq!(s.cursor, 10);
        s.new_line();
        assert_eq!(s.cursor, 90);
    }

    #[test]
    fn clear_and_pa_are_short_reads() {
        let mut s = logon();
        assert_eq!(s.press_aid(Aid::Clear).unwrap(), vec![0x6D]);
        assert!(!s.is_formatted());
        s.keyboard = Lock::Unlocked;
        assert_eq!(s.press_aid(Aid::Pa(1)).unwrap(), vec![0x6C]);
    }

    #[test]
    fn repeat_to_address_and_colors() {
        let mut s = Screen::new(24, 80, cp());
        let mut d = vec![0x00, ORDER_SA, XA_FOREGROUND, 0xF6, ORDER_RA];
        d.extend_from_slice(&addr(0, 5));
        d.push(cp().encode("-")[0]);
        s.write(&d, true);
        assert_eq!(&s.row_text(0)[..6], "----- ");
        assert_eq!(s.glyph(0).color, Color::Yellow);
        assert_eq!(s.keyboard, Lock::Unlocked); // no restore bit, was unlocked
    }

    #[test]
    fn eau_clears_input_and_unlocks() {
        let mut s = logon();
        for c in "ABC".chars() { s.type_char(c).unwrap(); }
        s.press_aid(Aid::Enter).unwrap();
        s.erase_all_unprotected();
        assert_eq!(s.keyboard, Lock::Unlocked);
        assert_eq!(&s.row_text(0)[10..13], "   ");
        assert!(s.row_text(0).contains("USERID"));
    }

    #[test]
    fn unformatted_read_modified() {
        let mut s = Screen::new(24, 80, cp());
        s.write(&[WCC_KEYBOARD_RESTORE], true);
        for c in "LOGON".chars() { s.type_char(c).unwrap(); }
        let rm = s.press_aid(Aid::Enter).unwrap();
        let mut want = vec![0x7D];
        want.extend_from_slice(&encode_address(5));
        want.extend(cp().encode("LOGON"));
        assert_eq!(rm, want);
    }

    #[test]
    fn glyphs_match_per_position_glyph() {
        let mut s = logon();
        for c in "ABC".chars() { s.type_char(c).unwrap(); }
        let all = s.glyphs();
        for (p, g) in all.iter().enumerate() {
            assert_eq!(*g, s.glyph(p), "position {p}");
        }
        assert!(s.text().starts_with(" USERID   ABC"));
    }

    #[test]
    fn sscp_lu_input() {
        let mut s = Screen::new(24, 80, cp());
        let mut msg = cp().encode("ENTER COMMAND");
        msg.push(0x15);
        s.sscp_write(&msg);
        assert_eq!(s.cursor, 80);
        for c in "TSO".chars() { s.type_char(c).unwrap(); }
        assert_eq!(s.sscp_input(), cp().encode("TSO"));
    }
}
