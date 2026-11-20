//! EBCDIC code pages.

use crate::ebcdic_tables::{CP037, CP1140, CP273, CP500};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CodePage {
    /// US/Canada (the usual North American default).
    #[default]
    Cp037,
    /// US/Canada with the euro sign.
    Cp1140,
    /// International (Belgium, Switzerland, Canada bilingual).
    Cp500,
    /// Germany/Austria.
    Cp273,
}

impl CodePage {
    pub const ALL: [CodePage; 4] = [CodePage::Cp037, CodePage::Cp1140, CodePage::Cp500, CodePage::Cp273];

    pub fn number(self) -> u16 {
        match self {
            CodePage::Cp037 => 37,
            CodePage::Cp1140 => 1140,
            CodePage::Cp500 => 500,
            CodePage::Cp273 => 273,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CodePage::Cp037 => "037 United States / Canada",
            CodePage::Cp1140 => "1140 United States / Canada (euro)",
            CodePage::Cp500 => "500 International",
            CodePage::Cp273 => "273 Germany / Austria",
        }
    }

    pub fn from_number(n: u16) -> Option<CodePage> {
        CodePage::ALL.into_iter().find(|c| c.number() == n)
    }

    fn table(self) -> &'static [char; 256] {
        match self {
            CodePage::Cp037 => &CP037,
            CodePage::Cp1140 => &CP1140,
            CodePage::Cp500 => &CP500,
            CodePage::Cp273 => &CP273,
        }
    }

    /// EBCDIC byte to a displayable character (controls become spaces).
    pub fn to_char(self, b: u8) -> char {
        match b {
            0x00..=0x3F => ' ',
            0xFF => ' ',
            _ => self.table()[b as usize],
        }
    }

    /// Character to an EBCDIC byte, if the code page has it.
    pub fn from_char(self, c: char) -> Option<u8> {
        if c == ' ' {
            return Some(0x40);
        }
        self.table().iter().position(|&t| t == c).map(|i| i as u8).filter(|&b| b >= 0x40 && b != 0xFF)
    }

    /// Text to EBCDIC, replacing unknown characters with '?'.
    pub fn encode(self, s: &str) -> Vec<u8> {
        s.chars().map(|c| self.from_char(c).unwrap_or(0x6F)).collect()
    }

    pub fn decode(self, b: &[u8]) -> String {
        b.iter().map(|&x| self.to_char(x)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp037_basics() {
        let cp = CodePage::Cp037;
        assert_eq!(cp.decode(&[0xC8, 0x85, 0x93, 0x93, 0x96]), "Hello");
        assert_eq!(cp.encode("[]¢¬|"), vec![0xBA, 0xBB, 0x4A, 0x5F, 0x4F]);
        assert_eq!(cp.to_char(0x00), ' ');
        assert_eq!(cp.from_char('\u{1}'), None);
        assert_eq!(CodePage::Cp1140.from_char('€'), Some(0x9F));
    }

    #[test]
    fn round_trip_printables() {
        for cp in CodePage::ALL {
            for b in 0x40u8..0xFF {
                let c = cp.to_char(b);
                assert_eq!(cp.from_char(c), Some(b), "{cp:?} byte {b:02X} char {c:?}");
            }
        }
    }
}
