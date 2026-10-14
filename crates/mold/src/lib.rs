// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! MoldUDP64 packet framing codec for the exchange8 vertical.
//!
//! The crate is the single tested definition of the MoldUDP64 downstream
//! packet grammar: it builds outbound multicast packets (header +
//! length-prefixed message blocks), parses inbound packets back into header
//! + block byte ranges, and formats both the count-0 heartbeat (carrying the
//! next-expected sequence) and the 20-byte retransmission request. It is a
//! pure library — no sockets, no database, no runtime of its own; session
//! sequencing, heartbeat timing, and transport belong to the consuming
//! services (Ex8:MD publisher, Ex8:VFeeds receive path).
//!
//! Byte layout (big-endian throughout):
//!
//! | Offset | Len | Field | Encoding |
//! |---|---|---|---|
//! | 0 | 10 | Session (ASCII, space-padded) | raw bytes |
//! | 10 | 8 | Sequence number | `u64` big-endian |
//! | 18 | 2 | Message count | `u16` big-endian |
//! | 20 | — | Message blocks | per block `u16` BE length + payload |
//!
//! Packets are capped at `MAX_MOLD_MSG_LENGTH` (1400 bytes).
//!
//! # Quick start
//!
//! ```
//! use exchange8_mold::{MoldPacket, parse};
//!
//! let mut pkt = MoldPacket::new("SESS01");
//! assert!(pkt.add_message(100, b"hello"));
//! let bytes = pkt.encode();
//!
//! let parsed = parse(&bytes).unwrap();
//! assert_eq!(parsed.session, "SESS01");
//! assert_eq!(parsed.sequence, 100);
//! assert_eq!(parsed.count, 1);
//! assert_eq!(&bytes[parsed.block_ranges[0].0..parsed.block_ranges[0].1], b"hello");
//! ```
//!
//! Design: `DESIGN.md` in this crate directory.

use thiserror::Error;

/// Maximum total packet size in bytes: session + header + all blocks must
/// fit inside one UDP datagram below typical MTU so senders never rely on
/// fragmentation (fragmented datagrams amplify loss).
///
/// `add_message` enforces the cap structurally for builder packets; external
/// producers must honor it themselves.
pub const MAX_MOLD_MSG_LENGTH: usize = 1400;

/// Fixed wire geometry constants of the MoldUDP64 header and block framing.
pub const SESSION_LEN: usize = 10;
/// Byte offset of the sequence-number field inside a packet.
pub const SEQ_OFFSET: usize = 10;
/// Size in bytes of the big-endian `u64` sequence-number field.
pub const SEQ_LEN: usize = 8;
/// Byte offset of the message-count field inside a packet.
pub const COUNT_OFFSET: usize = 18;
/// Size in bytes of the big-endian `u16` message-count field.
pub const COUNT_LEN: usize = 2;
/// Byte offset of the first message block (end of the fixed header).
pub const BLOCK_OFFSET: usize = 20;
/// Size in bytes of the big-endian `u16` length prefix that opens every
/// message block.
pub const BLOCK_LEN_LEN: usize = 2;

/// Typed failure surface of the codec — the complete set of outcomes for
/// parsing an inbound packet.
///
/// # Examples
///
/// ```
/// use exchange8_mold::{parse, MoldError};
///
/// let err = parse(&[0u8; 10]).unwrap_err();
/// assert!(matches!(err, MoldError::TooShort(10)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MoldError {
    /// The buffer is shorter than the fixed 20-byte header; the `usize` is
    /// the actual length seen.
    #[error("packet too short for header: {0} bytes")]
    TooShort(usize),
    /// A block (its length prefix at the given offset) declares more bytes
    /// than remain in the packet.
    #[error("block length at {0} exceeds packet")]
    BlockOverrun(usize),
    /// The packet exceeds [`MAX_MOLD_MSG_LENGTH`].
    #[error("packet exceeds {MAX_MOLD_MSG_LENGTH}")]
    TooLarge,
}

/// Builder for an outbound MoldUDP64 data packet.
///
/// Accumulates message blocks under a fixed 10-byte session identifier and
/// a packet-start sequence number, enforcing the [`MAX_MOLD_MSG_LENGTH`]
/// cap as blocks are added. Encoding is deterministic: the same builder
/// state always serializes to identical bytes.
///
/// # Examples
///
/// ```
/// use exchange8_mold::MoldPacket;
///
/// let mut pkt = MoldPacket::new("SESS01");
/// assert!(pkt.add_message(100, b"hello"));
/// assert_eq!(pkt.message_count(), 1);
/// let bytes = pkt.encode();
/// assert_eq!(bytes.len(), exchange8_mold::BLOCK_OFFSET + 2 + 5);
/// ```
#[derive(Debug, Clone)]
pub struct MoldPacket {
    /// Session identifier, ASCII, space-padded to exactly
    /// [`SESSION_LEN`] bytes.
    pub session: [u8; SESSION_LEN],
    /// Sequence number of the **first** message in the packet.
    pub sequence: u64,
    blocks: Vec<Vec<u8>>,
}

impl MoldPacket {
    /// Creates a new packet for a session.
    ///
    /// The session is copied as ASCII bytes and space-padded (with the
    /// `0x20` padding byte) to exactly [`SESSION_LEN`] bytes; longer
    /// strings are truncated to 10 bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_mold::MoldPacket;
    ///
    /// let mut pkt = MoldPacket::new("S");
    /// assert_eq!(&pkt.session[..1], b"S");
    /// assert!(pkt.session[1..].iter().all(|&b| b == b' '));
    /// ```
    pub fn new(session: &str) -> Self {
        let mut s = [ITCH_SPACE; SESSION_LEN];
        let b = session.as_bytes();
        let n = b.len().min(SESSION_LEN);
        s[..n].copy_from_slice(&b[..n]);
        MoldPacket {
            session: s,
            sequence: 0,
            blocks: Vec::new(),
        }
    }

    /// The space padding constant.
    const fn default_session() -> [u8; SESSION_LEN] {
        [ITCH_SPACE; SESSION_LEN]
    }

    /// Returns the number of bytes still available for message blocks
    /// within the [`MAX_MOLD_MSG_LENGTH`] packet cap.
    ///
    /// The accounting includes the 20-byte fixed header plus each stored
    /// block's 2-byte length prefix and payload.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_mold::{MoldPacket, BLOCK_OFFSET, BLOCK_LEN_LEN};
    ///
    /// let pkt = MoldPacket::new("S");
    /// assert_eq!(pkt.buffer_left(), exchange8_mold::MAX_MOLD_MSG_LENGTH - BLOCK_OFFSET);
    /// ```
    pub fn buffer_left(&self) -> usize {
        let used = BLOCK_OFFSET
            + self
                .blocks
                .iter()
                .map(|b| BLOCK_LEN_LEN + b.len())
                .sum::<usize>();
        MAX_MOLD_MSG_LENGTH.saturating_sub(used)
    }

    /// Adds a message payload with its sequence number.
    ///
    /// Behavioral rules (mirroring the protocol reference behavior):
    ///
    /// - An empty payload is the protocol's *null message* and is ignored:
    ///   the call returns `false` without mutating the packet.
    /// - The **first non-zero** sequence seen is captured as the packet's
    ///   [`sequence`](struct.MoldPacket.html) field; later calls do not
    ///   change it (blocks are consecutive from the packet start by
    ///   protocol contract).
    /// - When the block (2-byte length prefix + payload) would exceed the
    ///   packet cap, the call returns `false` and leaves the builder state
    ///   unchanged — the caller starts the next packet.
    ///
    /// # Returns
    ///
    /// `true` when the block was stored; `false` when refused (empty
    /// payload or insufficient remaining capacity).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_mold::MoldPacket;
    ///
    /// let mut pkt = MoldPacket::new("S");
    /// assert!(pkt.add_message(100, b"aaa"));
    /// assert!(pkt.add_message(101, b"bbb"));
    /// assert!(!pkt.add_message(102, &[])); // null message refused
    /// assert_eq!(pkt.message_count(), 2);
    /// ```
    pub fn add_message(&mut self, seq: u64, payload: &[u8]) -> bool {
        if payload.is_empty() {
            return false; // null message (seq 0) is ignored per protocol
        }
        let need = BLOCK_LEN_LEN + payload.len();
        if need > self.buffer_left() {
            return false;
        }
        if self.blocks.is_empty() && seq != 0 {
            self.sequence = seq;
        }
        self.blocks.push(payload.to_vec());
        true
    }

    /// Returns the current number of message blocks, i.e. the value
    /// written into the header's message-count field by [`encode`].
    ///
    /// [`encode`]: struct.MoldPacket.html#method.encode
    pub fn message_count(&self) -> u16 {
        self.blocks.len() as u16
    }

    /// Serializes the packet to its exact wire form.
    ///
    /// Layout: 10-byte space-padded session, `u64` big-endian sequence,
    /// `u16` big-endian message count, then per block a `u16` big-endian
    /// length followed by the payload. The `sequence` field is only
    /// meaningful when the count is non-zero — heartbeat packets carry the
    /// next-expected sequence explicitly. Output length never exceeds
    /// [`MAX_MOLD_MSG_LENGTH`] for builder packets (capacity is fit-checked
    /// in [`add_message`]).
    ///
    /// [`add_message`]: struct.MoldPacket.html#method.add_message
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(MAX_MOLD_MSG_LENGTH);
        out.extend_from_slice(&self.session);
        out.extend_from_slice(&self.sequence.to_be_bytes());
        out.extend_from_slice(&self.message_count().to_be_bytes());
        for b in &self.blocks {
            out.extend_from_slice(&(b.len() as u16).to_be_bytes());
            out.extend_from_slice(b);
        }
        out
    }

    /// Builds a heartbeat packet: a count-0 packet carrying the
    /// next-expected sequence.
    ///
    /// Heartbeats are emitted while a session is idle so receivers can
    /// distinguish a live-quiet feed from a lost one by observing the
    /// sequence advance. The output is exactly [`BLOCK_OFFSET`] (20) bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_mold::{MoldPacket, BLOCK_OFFSET};
    ///
    /// let hb = MoldPacket::heartbeat("SESS", 1234);
    /// assert_eq!(hb.len(), BLOCK_OFFSET);
    /// assert_eq!(&hb[10..18], &1234u64.to_be_bytes());
    /// assert_eq!(&hb[18..20], &0u16.to_be_bytes()); // count 0
    /// ```
    pub fn heartbeat(session: &str, next_seq: u64) -> Vec<u8> {
        let mut s = Self::default_session();
        let b = session.as_bytes();
        let n = b.len().min(SESSION_LEN);
        s[..n].copy_from_slice(&b[..n]);
        let mut out = Vec::with_capacity(BLOCK_OFFSET);
        out.extend_from_slice(&s);
        out.extend_from_slice(&next_seq.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out
    }
}

/// The ASCII space padding byte (0x20) used to pad session fields.
const ITCH_SPACE: u8 = 0x20;

/// The parsed view of an inbound MoldUDP64 packet.
///
/// Block payloads are deliberately **not** copied: [`block_ranges`] holds
/// `(start, end)` byte offsets into the original buffer, which the caller
/// (or the `exchange8::itch` codec) slices.
///
/// [`block_ranges`]: struct.ParsedPacket.html#structfield.block_ranges
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPacket {
    /// Session decoded from the 10-byte field, trailing spaces trimmed.
    pub session: String,
    /// Packet-start sequence number (big-endian `u64`).
    pub sequence: u64,
    /// Declared message-block count (big-endian `u16`).
    pub count: u16,
    /// One `(start, end)` byte-offset pair per block into the original
    /// packet buffer; payload bytes live at `packet[start..end]`.
    pub block_ranges: Vec<(usize, usize)>,
}

/// Parses a MoldUDP64 packet into its header values plus a block index.
///
/// The walk validates every declared block (2-byte big-endian length +
/// payload) against the actual buffer as it proceeds, so the first offending
/// block is reported with its offset. The result references the caller's
/// buffer by offset; no payload bytes are copied.
///
/// # Errors
///
/// Returns [`MoldError::TooShort`] when the buffer is smaller than the
/// 20-byte fixed header, and [`MoldError::BlockOverrun`] when a declared
/// block length extends beyond the packet.
///
/// # Examples
///
/// ```
/// use exchange8_mold::{MoldPacket, parse};
///
/// let mut pkt = MoldPacket::new("SESS01");
/// assert!(pkt.add_message(500, b"one"));
/// let bytes = pkt.encode();
///
/// let parsed = parse(&bytes).expect("well-formed packet");
/// assert_eq!(parsed.count, 1);
/// assert_eq!(&bytes[parsed.block_ranges[0].0..parsed.block_ranges[0].1], b"one");
/// ```
pub fn parse(packet: &[u8]) -> Result<ParsedPacket, MoldError> {
    if packet.len() < BLOCK_OFFSET {
        return Err(MoldError::TooShort(packet.len()));
    }
    let session = String::from_utf8_lossy(&packet[..SESSION_LEN])
        .trim_end_matches(ITCH_SPACE as char)
        .to_string();
    let sequence = u64::from_be_bytes(
        packet[SEQ_OFFSET..SEQ_OFFSET + SEQ_LEN]
            .try_into()
            .expect("8-byte slice"),
    );
    let count = u16::from_be_bytes(
        packet[COUNT_OFFSET..COUNT_OFFSET + COUNT_LEN]
            .try_into()
            .expect("2-byte slice"),
    );
    let mut block_ranges = Vec::with_capacity(count as usize);
    let mut pos = BLOCK_OFFSET;
    for _ in 0..count {
        if pos + BLOCK_LEN_LEN > packet.len() {
            return Err(MoldError::BlockOverrun(pos));
        }
        let len = u16::from_be_bytes([packet[pos], packet[pos + 1]]) as usize;
        let start = pos + BLOCK_LEN_LEN;
        let end = start + len;
        if end > packet.len() {
            return Err(MoldError::BlockOverrun(pos));
        }
        block_ranges.push((start, end));
        pos = end;
    }
    Ok(ParsedPacket {
        session,
        sequence,
        count,
        block_ranges,
    })
}

/// Builds a MoldUDP64 retransmission request (client → retransmit server):
/// `[session 10][start_seq u64 BE][count u16 BE]` — 20 bytes total.
///
/// A receiver detecting a sequence gap asks the retransmission service to
/// re-serve a range with this fixed frame; the request names the session,
/// the first missing sequence, and how many messages are wanted.
///
/// # Examples
///
/// ```
/// use exchange8_mold::{SESSION_LEN, retransmission_request};
///
/// let req = retransmission_request("SESS", 42, 3);
/// assert_eq!(req.len(), SESSION_LEN + 8 + 2); // 20 bytes
/// assert_eq!(&req[10..18], &42u64.to_be_bytes());
/// assert_eq!(&req[18..20], &3u16.to_be_bytes());
/// ```
pub fn retransmission_request(session: &str, start_seq: u64, count: u16) -> Vec<u8> {
    let mut s = [ITCH_SPACE; SESSION_LEN];
    let b = session.as_bytes();
    let n = b.len().min(SESSION_LEN);
    s[..n].copy_from_slice(&b[..n]);
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(&s);
    out.extend_from_slice(&start_seq.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_layout_golden() {
        let mut p = MoldPacket::new("SESS01");
        assert!(p.add_message(100, b"hello"));
        let bytes = p.encode();
        assert_eq!(bytes.len(), BLOCK_OFFSET + 2 + 5);
        // Session at 0..10, space-padded
        assert_eq!(&bytes[..6], b"SESS01");
        assert!(bytes[6..10].iter().all(|&b| b == ITCH_SPACE));
        // Sequence u64 BE at 10..18
        assert_eq!(bytes[10..18], 100u64.to_be_bytes());
        // Count u16 BE at 18..20
        assert_eq!(bytes[18..20], 1u16.to_be_bytes());
        // Block: len u16 BE + payload
        assert_eq!(bytes[20..22], 5u16.to_be_bytes());
        assert_eq!(&bytes[22..], b"hello");
    }

    #[test]
    fn first_seq_stored_only_once() {
        let mut p = MoldPacket::new("S");
        assert!(p.add_message(100, b"aaa"));
        assert!(p.add_message(101, b"bbb"));
        assert_eq!(p.sequence, 100); // first non-zero
        assert_eq!(p.message_count(), 2);
    }

    #[test]
    fn overflow_rejected() {
        let mut p = MoldPacket::new("S");
        let big = vec![0xAB; 1300];
        assert!(p.add_message(1, &big));
        // Only ~98 bytes left; a second 100-byte block won't fit.
        assert!(!p.add_message(2, &vec![0xCD; 100]));
        assert_eq!(p.message_count(), 1);
        // Null message ignored (platform spec).
        assert!(!p.add_message(3, &[]));
    }

    #[test]
    fn heartbeat_format() {
        let hb = MoldPacket::heartbeat("SESS", 1234);
        assert_eq!(hb.len(), BLOCK_OFFSET);
        assert_eq!(&hb[..4], b"SESS");
        assert_eq!(hb[10..18], 1234u64.to_be_bytes());
        assert_eq!(hb[18..20], 0u16.to_be_bytes()); // count 0
    }

    #[test]
    fn parse_round_trip() {
        let mut p = MoldPacket::new("SESS01");
        p.add_message(500, b"one");
        p.add_message(501, b"twotwo");
        let bytes = p.encode();
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.session, "SESS01");
        assert_eq!(parsed.sequence, 500);
        assert_eq!(parsed.count, 2);
        assert_eq!(
            &bytes[parsed.block_ranges[0].0..parsed.block_ranges[0].1],
            b"one"
        );
        assert_eq!(
            &bytes[parsed.block_ranges[1].0..parsed.block_ranges[1].1],
            b"twotwo"
        );
    }

    #[test]
    fn parse_truncated_rejected() {
        assert!(matches!(parse(&[0; 10]), Err(MoldError::TooShort(_))));
        let mut p = MoldPacket::new("S");
        p.add_message(1, b"xyz");
        let bytes = p.encode();
        // Cut inside the block payload → BlockOverrun.
        assert!(matches!(
            parse(&bytes[..23]),
            Err(MoldError::BlockOverrun(_))
        ));
        // Cut exactly at a block boundary parses fine with count 1.
        let partial = parse(&bytes[..22]);
        // 22 = header(20) + len(2) but no payload → overrun.
        assert!(matches!(partial, Err(MoldError::BlockOverrun(_))));
    }

    #[test]
    fn retrans_request_layout() {
        let r = retransmission_request("SESS", 42, 3);
        assert_eq!(r.len(), 20);
        assert_eq!(&r[..4], b"SESS");
        assert_eq!(r[10..18], 42u64.to_be_bytes());
        assert_eq!(r[18..20], 3u16.to_be_bytes());
    }
}
