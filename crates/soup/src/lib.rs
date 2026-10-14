// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! SoupBinTCP session-wire codec for exchange8 market-data sessions —
//! the login handshake, control frames, sequenced-data wrapping, and the
//! stream read support (`next_frame_len` / `split_frame`), exactly per
//! `design/behavior-catalog/wire-formats.json`:
//!
//! - Every message: `[2-byte length BE][1 type byte][payload]`, the
//!   length value counting the type byte (2-byte prefix excluded).
//! - Login Request (`L`): 47-byte payload — user[6]@3, pwd[10]@9,
//!   session[10]@19, seq[20 digits]@29 (frame offsets).
//! - Login Accept (`A`): 31-byte payload — session[10], seq[20].
//! - Login Reject (`J`): 2-byte payload — reason (`A` / `S`).
//! - Sequenced Data (`S`): `S` + ITCH payload.
//! - Heartbeats: server `H` (1-byte payload), client `R`;
//!   Unsequenced Data (`U`) and Logout (`O`) carry no payload.
//!
//! # Quick start
//!
//! ```
//! use exchange8_soup::{LoginRequest, next_frame_len, split_frame};
//!
//! let req = LoginRequest {
//!     username: "me".into(),
//!     password: "pw".into(),
//!     session: "SESS".into(),
//!     seqnumber: 50,
//! };
//! let frame = req.encode();
//! assert_eq!(next_frame_len(&frame), Some(frame.len()));
//! assert_eq!(split_frame(&frame).unwrap().0, b'L');
//! ```
//!
//! Detailed design: `crates/soup/DESIGN.md`.

use thiserror::Error;

/// Classic Soup line-termination byte (`0x0A`).
///
/// Surface constant reserved for text-style Soup exchanges; the binary
/// framing in this crate neither interprets nor emits it.
pub const LINE_FEED: u8 = 0x0A;

/// SoupBinTCP message type characters — the single letter following
/// the 2-byte length prefix in every frame.
///
/// Each constant is the ASCII value of one protocol letter; see
/// `crates/soup/DESIGN.md` §3 for the full frame grammar.
pub mod msg_type {
    /// Login Request (client → server).
    pub const LOGIN_REQUEST: u8 = b'L';
    /// Login Accepted (server → client).
    pub const LOGIN_ACCEPTED: u8 = b'A';
    /// Login Rejected (server → client).
    pub const LOGIN_REJECTED: u8 = b'J';
    /// Sequenced Data (server → client).
    pub const SEQUENCED_DATA: u8 = b'S';
    /// Server Heartbeat.
    pub const SERVER_HEARTBEAT: u8 = b'H';
    /// Client Heartbeat.
    pub const CLIENT_HEARTBEAT: u8 = b'R';
    /// Unsequenced Data (client → server).
    pub const UNSEQUENCED_DATA: u8 = b'U';
    /// Logout Request (client → server).
    pub const LOGOUT: u8 = b'O';
}

/// Login-reject reason characters for the `J` frame's 1-byte payload.
///
/// `A` when the caller is not authorized, `S` when the requested
/// session is not available.
pub mod reject_reason {
    /// Not authorized.
    pub const NOT_AUTHORIZED: u8 = b'A';
    /// Session not available.
    pub const NO_SESSION: u8 = b'S';
}

/// Exact Login Request payload length in bytes (type char included).
///
/// A login payload of any other length is rejected with
/// [`SoupError::BadLoginLen`].
pub const LOGIN_REQUEST_LEN: usize = 47;
/// Exact Login Accept payload length in bytes (type char included).
pub const LOGIN_ACCEPT_LEN: usize = 31;
/// Login Reject payload length in bytes (type char + reason byte).
pub const LOGIN_REJECT_LEN: usize = 2;
/// Total size of a payload-free control frame (`[2-byte len][type]`).
///
/// Covers the server heartbeat (`H`), client heartbeat (`R`), and
/// logout (`O`) frames, whose length value is always 1.
pub const SERVER_HEARTBEAT_FRAME: usize = 3;

/// Typed failures of the fallible parse/split calls — the crate's
/// whole error surface.
///
/// The frame-constructor functions are total and never return it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SoupError {
    /// A buffer shorter than the 3-byte minimum (length prefix + type
    /// byte) was passed where a complete frame is required.
    #[error("buffer too short: need {need}, have {have}")]
    TooShort { need: usize, have: usize },

    /// A login request/accept payload whose length differs from the
    /// exact catalog value (47 / 31 bytes).
    #[error("bad login request length {0}")]
    BadLoginLen(usize),

    /// Reserved for consumers that dispatch on type letters with an
    /// unknown-letter policy; no function in this crate constructs it.
    #[error("unknown message type {0:#x}")]
    UnknownType(u8),
}

/// Login Request payload: a subscriber's credentials and requested
/// replay-from sequence position.
///
/// The wire payload is exactly [`LOGIN_REQUEST_LEN`] bytes: username 6,
/// password 10, session 10, then a 20-character right-aligned decimal
/// sequence (frame offsets 3/9/19/29 per `wire-formats.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginRequest {
    /// Subscriber username; blank-padded to 6 bytes on the wire.
    pub username: String,
    /// Subscriber password; blank-padded to 10 bytes on the wire.
    pub password: String,
    /// Requested session name; blank-padded to 10 bytes on the wire.
    pub session: String,
    /// Sequence the client wants the stream to resume from (0 for a
    /// fresh session).
    pub seqnumber: u32,
}

impl LoginRequest {
    /// Decode the payload (the 47 bytes after the 2-byte length prefix).
    ///
    /// Payload layout (type-relative): type@0, user@1..7, pwd@7..17,
    /// session@17..27, seq(20 digits)@27..47 — frame-relative these are
    /// user@3, pwd@9, session@19, seq@29 (the `wire-formats.json` anchors).
    ///
    /// Fixed-width fields have trailing blanks trimmed; sequence text is
    /// decoded tolerantly (non-numeric text falls back to 0).
    ///
    /// # Errors
    /// Returns [`SoupError::BadLoginLen`] when `payload.len()` differs
    /// from [`LOGIN_REQUEST_LEN`] — the peer is then not a Soup endpoint
    /// or the frame is corrupt.
    ///
    /// # Examples
    /// ```
    /// use exchange8_soup::LoginRequest;
    ///
    /// let req = LoginRequest {
    ///     username: "me".into(),
    ///     password: "pw".into(),
    ///     session: "SESS".into(),
    ///     seqnumber: 50,
    /// };
    /// let payload = &req.encode()[2..];
    /// assert_eq!(LoginRequest::parse(payload).unwrap(), req);
    /// ```
    pub fn parse(payload: &[u8]) -> Result<Self, SoupError> {
        if payload.len() != LOGIN_REQUEST_LEN {
            return Err(SoupError::BadLoginLen(payload.len()));
        }
        let username = field_str(&payload[1..7]);
        let password = field_str(&payload[7..17]);
        let session = field_str(&payload[17..27]);
        let seq = std::str::from_utf8(&payload[27..47])
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        Ok(LoginRequest {
            username,
            password,
            session,
            seqnumber: seq,
        })
    }

    /// Encode the full frame `[len u16 BE][L][user][pwd][session][seq20]`
    /// — exactly `2 + [LOGIN_REQUEST_LEN]` = 49 bytes.
    ///
    /// Fixed-width fields are blank-padded (over-length inputs truncate
    /// to width); the sequence becomes 20 right-aligned decimal
    /// characters. Encoding is total for any `u32` sequence.
    ///
    /// # Examples
    /// ```
    /// use exchange8_soup::LoginRequest;
    ///
    /// let frame = LoginRequest {
    ///     username: "me".into(),
    ///     password: "pw".into(),
    ///     session: "SESS".into(),
    ///     seqnumber: 50,
    /// }
    /// .encode();
    /// assert_eq!(frame.len(), 49);
    /// assert_eq!(frame[2], b'L');
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 + LOGIN_REQUEST_LEN);
        out.extend_from_slice(&(LOGIN_REQUEST_LEN as u16).to_be_bytes());
        out.push(msg_type::LOGIN_REQUEST);
        push_field(&mut out, &self.username, 6);
        push_field(&mut out, &self.password, 10);
        push_field(&mut out, &self.session, 10);
        out.extend_from_slice(format!("{:>20}", self.seqnumber).as_bytes());
        out
    }
}

/// Login Accepted payload: the publisher's grant of a session and the
/// sequence the client's stream starts at.
///
/// The wire payload is exactly [`LOGIN_ACCEPT_LEN`] bytes: session 10,
/// then a 20-character right-aligned decimal sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginAccept {
    /// Granted session name; blank-padded to 10 bytes on the wire.
    pub session: String,
    /// First sequence number the client will receive.
    pub seqnumber: u32,
}

impl LoginAccept {
    /// Build the full frame — `[len u16 BE][A][session 10][seq 20]`,
    /// exactly `2 + [LOGIN_ACCEPT_LEN]` = 33 bytes.
    ///
    /// # Examples
    /// ```
    /// use exchange8_soup::LoginAccept;
    ///
    /// let frame = LoginAccept { session: "S".into(), seqnumber: 1 }.encode();
    /// assert_eq!(frame.len(), 33);
    /// assert_eq!(frame[2], b'A');
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 + LOGIN_ACCEPT_LEN);
        out.extend_from_slice(&(LOGIN_ACCEPT_LEN as u16).to_be_bytes());
        out.push(msg_type::LOGIN_ACCEPTED);
        push_field(&mut out, &self.session, 10);
        out.extend_from_slice(format!("{:>20}", self.seqnumber).as_bytes());
        out
    }

    /// Decode the payload (the 31 bytes after the 2-byte length
    /// prefix); inverse of [`LoginAccept::encode`].
    ///
    /// # Errors
    /// Returns [`SoupError::BadLoginLen`] when `payload.len()` differs
    /// from [`LOGIN_ACCEPT_LEN`] (31).
    ///
    /// # Examples
    /// ```
    /// use exchange8_soup::LoginAccept;
    ///
    /// let acc = LoginAccept { session: "S".into(), seqnumber: 1 };
    /// let payload = &acc.encode()[2..];
    /// assert_eq!(LoginAccept::parse(payload).unwrap(), acc);
    /// ```
    pub fn parse(payload: &[u8]) -> Result<Self, SoupError> {
        if payload.len() != LOGIN_ACCEPT_LEN {
            return Err(SoupError::BadLoginLen(payload.len()));
        }
        // type@0, session@1..11, seq@11..31 (payload-relative).
        Ok(LoginAccept {
            session: field_str(&payload[1..11]),
            seqnumber: std::str::from_utf8(&payload[11..31])
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0),
        })
    }
}

/// Build the login-rejected frame — `[len u16 BE][J][reason]`, exactly
/// 4 bytes.
///
/// `reason` passes through verbatim; the protocol letters live in
/// [`reject_reason`] (`A` not authorized, `S` no session).
///
/// # Examples
/// ```
/// use exchange8_soup::{login_rejected, reject_reason};
///
/// let frame = login_rejected(reject_reason::NOT_AUTHORIZED);
/// assert_eq!(frame.len(), 4);
/// assert_eq!(frame[2], b'J');
/// assert_eq!(frame[3], b'A');
/// ```
pub fn login_rejected(reason: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    out.extend_from_slice(&(LOGIN_REJECT_LEN as u16).to_be_bytes());
    out.push(msg_type::LOGIN_REJECTED);
    out.push(reason);
    out
}

/// Build the server heartbeat frame (`[len u16 BE][H]`, 3 bytes) that
/// a publisher emits to keep an idle session alive.
pub fn server_heartbeat() -> Vec<u8> {
    let mut out = Vec::with_capacity(SERVER_HEARTBEAT_FRAME);
    out.extend_from_slice(&1u16.to_be_bytes());
    out.push(msg_type::SERVER_HEARTBEAT);
    out
}

/// Build the client heartbeat frame (`[len u16 BE][R]`, 3 bytes) that
/// a subscriber emits to keep an idle session alive.
pub fn client_heartbeat() -> Vec<u8> {
    let mut out = Vec::with_capacity(SERVER_HEARTBEAT_FRAME);
    out.extend_from_slice(&1u16.to_be_bytes());
    out.push(msg_type::CLIENT_HEARTBEAT);
    out
}

/// Build the logout frame (`[len u16 BE][O]`, 3 bytes) with which a
/// client requests a clean disconnect.
pub fn logout() -> Vec<u8> {
    let mut out = Vec::with_capacity(SERVER_HEARTBEAT_FRAME);
    out.extend_from_slice(&1u16.to_be_bytes());
    out.push(msg_type::LOGOUT);
    out
}

/// Wrap a market-data payload into a Sequenced Data frame —
/// `[1+len u16 BE][S][payload]`, `2 + 1 + payload.len()` bytes.
///
/// The payload is carried byte-opaque; ITCH decoding belongs to
/// `exchange8::itch`, never the framing layer.
///
/// # Examples
/// ```
/// use exchange8_soup::{next_frame_len, sequenced_data};
///
/// let frame = sequenced_data(b"AB");
/// assert_eq!(next_frame_len(&frame), Some(5));
/// assert_eq!(frame[2], b'S');
/// ```
pub fn sequenced_data(itch_payload: &[u8]) -> Vec<u8> {
    let len = 1 + itch_payload.len();
    let mut out = Vec::with_capacity(2 + len);
    out.extend_from_slice(&(len as u16).to_be_bytes());
    out.push(msg_type::SEQUENCED_DATA);
    out.extend_from_slice(itch_payload);
    out
}

/// Total frame length announced by a buffer's 2-byte big-endian
/// length prefix, if decodable.
///
/// Returns `None` while fewer than 2 bytes are available — the normal
/// "more bytes will arrive" signal for an accumulating reader, never an
/// error. The value counts prefix + type byte + payload.
///
/// # Examples
/// ```
/// use exchange8_soup::{next_frame_len, sequenced_data};
///
/// assert_eq!(next_frame_len(&[0x00]), None);
/// let frame = sequenced_data(b"AB");
/// assert_eq!(next_frame_len(&frame), Some(frame.len()));
/// ```
pub fn next_frame_len(buf: &[u8]) -> Option<usize> {
    if buf.len() < 2 {
        return None;
    }
    let payload = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    Some(2 + payload)
}

/// Split a complete frame into its type byte and payload slice.
///
/// Payload-free control frames (heartbeats, logout) split to an empty
/// slice, never an error.
///
/// # Errors
/// Returns [`SoupError::TooShort`] when the frame is shorter than the
/// 3-byte minimum (length prefix + type byte); accumulate more bytes
/// first using [`next_frame_len`].
///
/// # Examples
/// ```
/// use exchange8_soup::{logout, split_frame};
///
/// let frame = logout();
/// let (type_byte, payload) = split_frame(&frame).unwrap();
/// assert_eq!(type_byte, b'O');
/// assert!(payload.is_empty());
/// ```
pub fn split_frame(frame: &[u8]) -> Result<(u8, &[u8]), SoupError> {
    if frame.len() < 3 {
        return Err(SoupError::TooShort {
            need: 3,
            have: frame.len(),
        });
    }
    Ok((frame[2], &frame[3..]))
}

// Fixed-width field decode: lossy-UTF-8 view with trailing blanks
// trimmed (the parse half of the field round trip; `DESIGN.md` §3).
fn field_str(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim_end().to_string()
}

// Fixed-width field encode: copy up to `width` bytes, pad the rest
// with blanks (the encode half of the field round trip; `DESIGN.md` §3).
fn push_field(out: &mut Vec<u8>, value: &str, width: usize) {
    let b = value.as_bytes();
    let n = b.len().min(width);
    out.extend_from_slice(&b[..n]);
    for _ in n..width {
        out.push(b' ');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden: Login Request layout — anchored to `wire-formats.json`
    /// `soupbin_login`: total 47 payload, user@3, pwd@9, session@19, seq@29.
    #[test]
    fn login_request_golden() {
        let req = LoginRequest {
            username: "trader".into(),
            password: "secret".into(),
            session: "SESS01".into(),
            seqnumber: 100,
        };
        let frame = req.encode();
        // 2-byte length + 47-byte payload
        assert_eq!(frame.len(), 2 + 47);
        assert_eq!(&frame[..2], &47u16.to_be_bytes());
        assert_eq!(frame[2], b'L');
        assert_eq!(&frame[3..9], b"trader");
        assert_eq!(&frame[9..19], b"secret    ");
        assert_eq!(&frame[19..29], b"SESS01    ");
        // 20-digit right-aligned seq
        assert_eq!(&frame[29..49], b"                 100");
        // Round trip
        let back = LoginRequest::parse(&frame[2..]).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn login_bad_length_rejected() {
        assert!(matches!(
            LoginRequest::parse(&[0; 10]),
            Err(SoupError::BadLoginLen(_))
        ));
    }

    #[test]
    fn login_accept_layout() {
        let a = LoginAccept {
            session: "S1".into(),
            seqnumber: 7,
        };
        let f = a.encode();
        assert_eq!(f.len(), 2 + 31);
        assert_eq!(&f[..2], &31u16.to_be_bytes());
        assert_eq!(f[2], b'A');
        assert_eq!(&f[3..13], b"S1        ");
        assert_eq!(&f[13..33], b"                   7");
        let back = LoginAccept::parse(&f[2..]).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn control_frames() {
        assert_eq!(server_heartbeat().len(), 3);
        assert_eq!(server_heartbeat()[..2], 1u16.to_be_bytes()[..]);
        assert_eq!(server_heartbeat()[2], b'H');
        assert_eq!(client_heartbeat()[2], b'R');
        assert_eq!(logout()[2], b'O');
        let lr = login_rejected(reject_reason::NOT_AUTHORIZED);
        assert_eq!(lr.len(), 4);
        assert_eq!(&lr[..2], &2u16.to_be_bytes());
        assert_eq!(lr[3], b'A');
    }

    #[test]
    fn sequenced_data_wraps_itch() {
        let f = sequenced_data(&[0x41, 0x42, 0x43]);
        assert_eq!(&f[..2], &4u16.to_be_bytes());
        assert_eq!(f[2], b'S');
        assert_eq!(&f[3..], b"ABC");
        assert_eq!(next_frame_len(&f), Some(f.len()));
        let (t, payload) = split_frame(&f).unwrap();
        assert_eq!(t, b'S');
        assert_eq!(payload, b"ABC");
    }

    #[test]
    fn partial_frame_needs_more() {
        assert_eq!(next_frame_len(&[0x00]), None);
        let f = sequenced_data(b"xy");
        assert_eq!(next_frame_len(&f[..1]), None);
    }
}
