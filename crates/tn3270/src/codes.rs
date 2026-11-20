//! 3270 data stream constants.

// Commands (both the local-attach and the SNA/remote codes appear in practice).
pub const CMD_W: [u8; 2] = [0x01, 0xF1];
pub const CMD_EW: [u8; 2] = [0x05, 0xF5];
pub const CMD_EWA: [u8; 2] = [0x0D, 0x7E];
pub const CMD_RB: [u8; 2] = [0x02, 0xF2];
pub const CMD_RM: [u8; 2] = [0x06, 0xF6];
pub const CMD_RMA: [u8; 2] = [0x0E, 0x6E];
pub const CMD_EAU: [u8; 2] = [0x0F, 0x6F];
pub const CMD_WSF: [u8; 2] = [0x11, 0xF3];
pub const CMD_NOP: u8 = 0x03;

// Orders
pub const ORDER_PT: u8 = 0x05;
pub const ORDER_GE: u8 = 0x08;
pub const ORDER_SBA: u8 = 0x11;
pub const ORDER_EUA: u8 = 0x12;
pub const ORDER_IC: u8 = 0x13;
pub const ORDER_SF: u8 = 0x1D;
pub const ORDER_SA: u8 = 0x28;
pub const ORDER_SFE: u8 = 0x29;
pub const ORDER_MF: u8 = 0x2C;
pub const ORDER_RA: u8 = 0x3C;

// WCC bits
pub const WCC_RESET_MDT: u8 = 0x01;
pub const WCC_KEYBOARD_RESTORE: u8 = 0x02;
pub const WCC_ALARM: u8 = 0x04;

// Field attribute bits
pub const FA_PROTECTED: u8 = 0x20;
pub const FA_NUMERIC: u8 = 0x10;
pub const FA_INTENSITY: u8 = 0x0C;
pub const FA_INT_HIGH: u8 = 0x08;
pub const FA_INT_ZERO: u8 = 0x0C;
pub const FA_MDT: u8 = 0x01;

// Extended attribute types
pub const XA_ALL: u8 = 0x00;
pub const XA_3270: u8 = 0xC0;
pub const XA_HIGHLIGHT: u8 = 0x41;
pub const XA_FOREGROUND: u8 = 0x42;

// Special characters
pub const EBC_NULL: u8 = 0x00;
pub const EBC_SPACE: u8 = 0x40;
pub const EBC_DUP: u8 = 0x1C;
pub const EBC_FM: u8 = 0x1E;

/// The 64 codes used for 12-bit addresses and attribute bytes.
pub const CODE_TABLE: [u8; 64] = [
    0x40, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F,
    0x50, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F,
    0x60, 0x61, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F,
    0xF0, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F,
];

pub fn decode_address(b1: u8, b2: u8) -> usize {
    if b1 & 0xC0 == 0 {
        (((b1 & 0x3F) as usize) << 8) | b2 as usize // 14-bit
    } else {
        (((b1 & 0x3F) as usize) << 6) | (b2 & 0x3F) as usize // 12-bit
    }
}

pub fn encode_address(addr: usize) -> [u8; 2] {
    if addr < 4096 {
        [CODE_TABLE[(addr >> 6) & 0x3F], CODE_TABLE[addr & 0x3F]]
    } else {
        [((addr >> 8) & 0x3F) as u8, (addr & 0xFF) as u8]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_round_trip() {
        for a in [0, 1, 79, 80, 1919, 3439, 4095, 4096, 9919] {
            let [b1, b2] = encode_address(a);
            assert_eq!(decode_address(b1, b2), a);
        }
        // Row 2, col 1 is address 80 = 0xC1 0x50 in 12-bit form.
        assert_eq!(encode_address(80), [0xC1, 0x50]);
    }
}
