// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 transport primitives - service-agnostic wiring for the
//! internal direct-stream tier (master design §5.2/§7.2).
//!
//! Contents:
//! - framed codec (4-byte LE length + JSON): `encode`/`decode`/`next_frame_len`
//! - bounded in-process MPSC channels: `channel`/`BoundedSender`/`Stream`
//! - id-lease blocks: `SequenceLease` (in-process windows filled from the
//!   platform sequence-lease facility - see the data-model design §2a)
//!
//! Nothing here is tied to one service identity, so the crate sits at one
//! facade level (`exchange8::wire`) per principles §2.1. SoupBinTCP has its
//! own framing in `crates/soup` - distinct wire format, kept separate.
//!
//! # Quick start
//!
//! ```rust
//! use exchange8_wire::{channel, encode, decode, SequenceLease};
//!
//! // Frame a value and read it back.
//! let frame = encode(&"hello");
//! let value: String = decode(&frame).unwrap();
//! assert_eq!(value, "hello");
//!
//! // Hand out ids from an in-process lease block (the host service refills
//! // it from the platform sequence-lease facility when exhausted).
//! let mut lease = SequenceLease::new("env-1", "me", "trade", 100, 100 + SequenceLease::BATCH);
//! assert_eq!(lease.take(), Some(100));
//!
//! // Bounded in-process hand-off: a full queue returns the value.
//! let (tx, _rx) = channel::<u64>(1);
//! assert!(tx.try_send(1).is_ok());
//! ```
//!
//! Design: `design/sentin8-exchange8-technical-design.md` §4.1, §5.2;
//! `crates/wire/DESIGN.md`.

use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

/// Wire framing errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    #[error("frame too short for length prefix")]
    TooShort,
    #[error("frame length {0} exceeds maximum")]
    TooLarge(usize),
    #[error("payload is not valid JSON: {0}")]
    Json(String),
}

/// Maximum frame payload (4 MiB) - guards against corrupt length prefixes.
pub const MAX_FRAME: usize = 4 * 1024 * 1024;

/// Encode `value` into a framed buffer: 4-byte LE length + JSON.
pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    let body = serde_json::to_vec(value).expect("serialize cannot fail for domain types");
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Expected total frame length given the 4-byte prefix.
pub fn next_frame_len(prefix: &[u8]) -> Option<usize> {
    if prefix.len() < 4 {
        return None;
    }
    let n = u32::from_le_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;
    Some(n + 4)
}

/// Decode a complete frame (prefix included) into `T`.
pub fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<T, DecodeError> {
    if frame.len() < 4 {
        return Err(DecodeError::TooShort);
    }
    let n = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if n > MAX_FRAME {
        return Err(DecodeError::TooLarge(n));
    }
    if frame.len() < n + 4 {
        return Err(DecodeError::TooShort);
    }
    serde_json::from_slice(&frame[4..4 + n]).map_err(|e| DecodeError::Json(e.to_string()))
}

/// Try-send error carrying the value back (backpressure, never panics).
#[derive(Debug, PartialEq, Eq)]
pub struct SendError<T>(pub T);

impl<T> SendError<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Bounded MPSC sender (sync `try_send` semantics for hot paths).
///
/// Clonable across producer tasks; the receiver side (`Stream`) stays a
/// single consumer.
#[derive(Clone)]
pub struct BoundedSender<T> {
    tx: tokio::sync::mpsc::Sender<T>,
}

impl<T> BoundedSender<T> {
    /// Attempt a non-blocking send; returns the value on a full channel.
    ///
    /// A closed receiver is indistinguishable from a full one in the type -
    /// the value is handed back either way.
    pub fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        match self.tx.try_send(value) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(v)) => Err(SendError(v)),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(v)) => Err(SendError(v)),
        }
    }

    /// Await a free queue slot and send; the value is handed back when the
    /// receiver is closed.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.tx.send(value).await.map_err(|e| SendError(e.0))
    }

    /// Report the configured queue depth.
    pub fn capacity(&self) -> usize {
        self.tx.capacity()
    }
}

/// Owned stream side of a bounded channel.
pub struct Stream<T> {
    rx: tokio::sync::mpsc::Receiver<T>,
}

impl<T> Stream<T> {
    /// Await the next item; `None` once all senders are dropped.
    pub async fn recv(&mut self) -> Option<T> {
        self.rx.recv().await
    }

    /// Try to take an item without blocking.
    pub fn try_recv(&mut self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

/// Create a bounded channel pair.
///
/// `queue_depth` is floored at 1 so `try_send` always has a well-defined
/// success case.
pub fn channel<T>(queue_depth: usize) -> (BoundedSender<T>, Stream<T>) {
    let (tx, rx) = tokio::sync::mpsc::channel(queue_depth.max(1));
    (BoundedSender { tx }, Stream { rx })
}

/// An in-process lease block of ids from a platform sequence lease.
///
/// The block covers the half-open id window `[next, end)` for one identity
/// triple (`env_id`, `service`, `seq_type` - e.g. `('env-1', 'me', 'trade')`).
/// The host service fills blocks from the platform sequence-lease facility
/// (disjoint batch leases; see the vertical data-model design §2a) and re-arms
/// hand-out here when a block is exhausted.
#[derive(Debug, Clone)]
pub struct SequenceLease {
    pub env_id: String,
    pub service: String,
    pub seq_type: String,
    /// Inclusive start.
    pub next: u64,
    /// Exclusive end.
    pub end: u64,
}

impl SequenceLease {
    /// Default ids per lease block.
    pub const BATCH: u64 = 10_000;

    /// Arm a lease block over `next..end` (end exclusive).
    pub fn new(env_id: &str, service: &str, seq_type: &str, next: u64, end: u64) -> Self {
        SequenceLease {
            env_id: env_id.into(),
            service: service.into(),
            seq_type: seq_type.into(),
            next,
            end,
        }
    }

    /// Take the next id from the lease; `None` when exhausted.
    ///
    /// Ids hand out strictly increasing; exhaustion never yields an id
    /// beyond the block.
    pub fn take(&mut self) -> Option<u64> {
        if self.next < self.end {
            let id = self.next;
            self.next += 1;
            Some(id)
        } else {
            None
        }
    }

    /// Ids left in the current block.
    pub fn remaining(&self) -> u64 {
        self.end - self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip() {
        let payload = "hello";
        let frame = encode(&payload);
        assert_eq!(frame.len(), 4 + 7);
        assert_eq!(next_frame_len(&frame[..4]), Some(frame.len()));
        let back: String = decode(&frame).unwrap();
        assert_eq!(back, "hello");
    }

    #[test]
    fn frame_too_short() {
        let e = decode::<String>(&[1, 2, 3]).unwrap_err();
        assert!(matches!(e, DecodeError::TooShort));
    }

    #[test]
    fn bounded_channel_backpressure() {
        let (tx, mut rx) = channel::<u32>(2);
        assert!(tx.try_send(1).is_ok());
        assert!(tx.try_send(2).is_ok());
        // Full → value returned.
        let err = tx.try_send(3).unwrap_err();
        assert_eq!(err.into_inner(), 3);
        assert_eq!(rx.try_recv(), Some(1));
        assert!(tx.try_send(4).is_ok());
    }

    #[test]
    fn sequence_lease() {
        let mut lease = SequenceLease::new("e", "me", "trade", 100, 100 + SequenceLease::BATCH);
        assert_eq!(lease.take(), Some(100));
        assert_eq!(lease.take(), Some(101));
        assert_eq!(lease.remaining(), 9_998);
        let mut tiny = SequenceLease::new("e", "me", "order", 5, 6);
        assert_eq!(tiny.take(), Some(5));
        assert_eq!(tiny.take(), None);
    }
}
