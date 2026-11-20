//! Query Reply: tells the host what this terminal supports, in answer to a
//! Read Partition (Query) structured field.

const QR: u8 = 0x81;
const QC_SUMMARY: u8 = 0x80;
const QC_USABLE_AREA: u8 = 0x81;
const QC_COLOR: u8 = 0x86;
const QC_HIGHLIGHT: u8 = 0x87;
const QC_REPLY_MODES: u8 = 0x88;
const QC_IMPLICIT_PARTITION: u8 = 0xA6;

fn sf(body: &[u8]) -> Vec<u8> {
    let len = (body.len() + 2) as u16;
    let mut v = len.to_be_bytes().to_vec();
    v.extend_from_slice(body);
    v
}

/// The full inbound Query Reply (starting with AID 0x88).
pub fn query_reply(rows: u16, cols: u16) -> Vec<u8> {
    let codes = [QC_SUMMARY, QC_USABLE_AREA, QC_IMPLICIT_PARTITION, QC_COLOR, QC_HIGHLIGHT, QC_REPLY_MODES];
    let mut v = vec![0x88];

    let mut summary = vec![QR, QC_SUMMARY];
    summary.extend_from_slice(&codes);
    v.extend(sf(&summary));

    let mut ua = vec![QR, QC_USABLE_AREA, 0x01, 0x00];
    ua.extend_from_slice(&cols.to_be_bytes());
    ua.extend_from_slice(&rows.to_be_bytes());
    ua.extend_from_slice(&[0x01, 0x00, 0x0A, 0x02, 0xE5, 0x00, 0x02, 0x00, 0x6F, 0x09, 0x0C]);
    ua.extend_from_slice(&(rows * cols).to_be_bytes());
    v.extend(sf(&ua));

    let mut ip = vec![QR, QC_IMPLICIT_PARTITION, 0x00, 0x00, 0x0B, 0x01, 0x00];
    ip.extend_from_slice(&80u16.to_be_bytes());
    ip.extend_from_slice(&24u16.to_be_bytes());
    ip.extend_from_slice(&cols.to_be_bytes());
    ip.extend_from_slice(&rows.to_be_bytes());
    v.extend(sf(&ip));

    let mut color = vec![QR, QC_COLOR, 0x00, 0x10, 0x00, 0xF4];
    for c in 0xF1..=0xFFu8 {
        color.extend_from_slice(&[c, c]);
    }
    v.extend(sf(&color));

    v.extend(sf(&[QR, QC_HIGHLIGHT, 0x05, 0x00, 0xF0, 0xF1, 0xF1, 0xF2, 0xF2, 0xF4, 0xF4, 0xF8, 0xF8]));
    v.extend(sf(&[QR, QC_REPLY_MODES, 0x00, 0x01, 0x02]));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_are_consistent() {
        let q = query_reply(24, 80);
        assert_eq!(q[0], 0x88);
        let mut i = 1;
        let mut ids = Vec::new();
        while i < q.len() {
            let len = u16::from_be_bytes([q[i], q[i + 1]]) as usize;
            assert!(len >= 4);
            ids.push(q[i + 3]);
            i += len;
        }
        assert_eq!(i, q.len());
        assert_eq!(ids, vec![0x80, 0x81, 0xA6, 0x86, 0x87, 0x88]);
    }
}
