// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 ME client — typed client library for the Ex8:ME direct
//! streams (order entry + lifecycle + recovery).
//!
//! Two functions over one shape: `encode_envelope` / `decode_envelope`
//! turn `exchange8::contracts::Envelope<T>` values into `exchange8::wire`
//! frames and back — the typed codec surface for the point-to-point
//! trading hot path (ME ↔ Gateway ↔ OMS), which rides Sentin8:TLS with
//! cert-bound M2M tokens instead of the inbox-first bus (master design
//! §7.2). The same envelope shape feeds the bus mirror, so a message is
//! portable between the two transport tiers without reshaping.
//!
//! Scope: ME-stream client concerns only. Service-agnostic transport
//! primitives (framed codec, bounded channels, sequence leases) live in
//! `exchange8::wire`; the Gateway → Risk authorize DTOs live in
//! `exchange8::contracts::risk` — cross-service API surface belongs to the
//! shared contracts crate (principles §2.1, master design §4.1).
//!
//! # Quickstart
//!
//! ```
//! use exchange8_contracts::{Envelope, KillReason, OrderKilled};
//! use exchange8_me_client::{decode_envelope, encode_envelope};
//!
//! let env = Envelope::<OrderKilled> {
//!     event_type: "exchange8:me:order:killed".into(),
//!     env_id: "bank-ops".into(),
//!     seq: 3,
//!     idempotency_key: "me-7-42".into(),
//!     payload: OrderKilled {
//!         order_id: 42,
//!         sub_order_id: 1,
//!         instrument_id: 7,
//!         connection_id: 3,
//!         reason: KillReason::Ioc,
//!     },
//! };
//! let frame = encode_envelope(&env);
//! let back = decode_envelope::<OrderKilled>(&frame).unwrap();
//! assert_eq!(back.seq, 3);
//! assert_eq!(back.payload.order_id, 42);
//! ```
//!
//! Design: `crates/me/client/DESIGN.md`; transport tiers in
//! `design/sentin8-exchange8-technical-design.md` §7.2.

use exchange8_contracts::Envelope;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Encode a typed envelope into a wire frame (`exchange8::wire` codec).
///
/// The *whole envelope* is the unit of encoding — `event_type`, `env_id`,
/// `seq`, `idempotency_key` and the payload travel together, never the
/// payload alone, so tenant attribution, ordering and idempotency stay
/// inside the one shape both transport tiers (direct stream and bus
/// mirror) already share. The frame layout is exactly the
/// `exchange8::wire` codec's (4-byte little-endian length prefix + JSON,
/// bounded by `MAX_FRAME`); the crate adds no bytes of its own, which is
/// what makes recorded sessions replay byte-identically.
///
/// # Examples
/// ```
/// use exchange8_contracts::Envelope;
/// use exchange8_me_client::{decode_envelope, encode_envelope};
///
/// let env = Envelope::<u64> {
///     event_type: "exchange8:me:order:added".into(),
///     env_id: "env-1".into(),
///     seq: 1,
///     idempotency_key: "me-1".into(),
///     payload: 42,
/// };
/// let frame = encode_envelope(&env);
/// let back = decode_envelope::<u64>(&frame).unwrap();
/// assert_eq!(back.payload, 42);
/// ```
pub fn encode_envelope<T: Serialize>(value: &Envelope<T>) -> Vec<u8> {
    exchange8_wire::encode(value)
}

/// Decode a wire frame into a typed envelope.
///
/// Framing errors surface as the `exchange8::wire::DecodeError` taxonomy
/// (`TooShort`, `TooLarge`, `Json`) — never a panic, never silent
/// truncation. Recovery policy belongs to the consumer: a direct-stream
/// reader typically resynchronizes on the next length-validated frame
/// (`exchange8::wire::next_frame_len`) or tears the stream down for a
/// rejoin/replay.
///
/// # Examples
/// ```
/// use exchange8_contracts::Envelope;
/// use exchange8_me_client::{decode_envelope, encode_envelope};
///
/// let env = Envelope::<u64> {
///     event_type: "exchange8:me:trade:executed".into(),
///     env_id: "env-1".into(),
///     seq: 7,
///     idempotency_key: "me-7".into(),
///     payload: 99,
/// };
/// let frame = encode_envelope(&env);
/// let back = decode_envelope::<u64>(&frame).unwrap();
/// assert_eq!(back.seq, 7);
/// ```
///
/// # Errors
/// Returns the codec's [`exchange8_wire::DecodeError`] unchanged:
/// `TooShort` for a truncated frame, `TooLarge` for a payload over
/// `MAX_FRAME`, `Json` for an unparseable payload. No other error type is
/// introduced and no error is swallowed.
pub fn decode_envelope<T: DeserializeOwned>(
    frame: &[u8],
) -> Result<Envelope<T>, exchange8_wire::DecodeError> {
    exchange8_wire::decode(frame)
}
