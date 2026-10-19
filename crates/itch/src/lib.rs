// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Byte-exact encoders for the venue's proprietary **ITCH VER2** market-data
//! wire format - both the binary packet forms and the fixed-width text forms.
//!
//! **This is NOT the standard NASDAQ ITCH 4.1 spec.** It is the proprietary
//! VER2 layout; every length, field order, and pad byte in this crate is
//! pinned to `design/behavior-catalog/wire-formats.json`, and the golden
//! tests anchor there - not to the public spec.
//!
//! Quick start (encode a message and hand it to a framing crate):
//! ```
//! use exchange8_itch::{AddOrder, msg_type, msg_len};
//!
//! let order = AddOrder {
//!     side: b'B',
//!     instrument_id: 7,
//!     order_id: 42,
//!     shares: 100,
//!     price: 101_000,
//!     broker: 3,
//!     book_id: b'O',
//! };
//! let bytes = order.encode();
//! assert_eq!(bytes[0], msg_type::ADD_ORDER);
//! assert_eq!(bytes.len(), msg_len::ADD_ORDER);
//! ```
//!
//! All multi-byte numeric fields are **big-endian**; text fields are
//! space-padded ([`ITCH_SPACE`] = `0x20`). Field declaration order in a
//! message struct equals wire order.
//!
//! Design: `crates/itch/DESIGN.md`; overview: `crates/itch/README.md`.

use thiserror::Error;

/// The universal pad byte (`0x20`): every reserved or padding field on the
/// wire holds this byte, in both the binary and the text message forms.
pub const ITCH_SPACE: u8 = 0x20;

/// Book-identifier letter codes (catalog `book_id_codes`).
pub mod book_id {
    /// `'D'` dark pool
    pub const DARK_POOL: u8 = b'D';
    /// `'L'` lit
    pub const LIT: u8 = b'L';
    /// `'Q'` QLX
    pub const QLX: u8 = b'Q';
    /// `'O'` MEMP - the venue default book
    pub const MEMP: u8 = b'O';
}

/// Message type letters: byte 0 of every encoded VER2 message.
pub mod msg_type {
    /// `A` - Add Order (20 B binary).
    pub const ADD_ORDER: u8 = b'A';
    /// `E` - Order Executed (16 B binary).
    pub const ORDER_EXECUTED: u8 = b'E';
    /// `U` - Order Replaced (20 B binary).
    pub const ORDER_REPLACED: u8 = b'U';
    /// `D` - Order Delete (8 B binary).
    pub const ORDER_DELETE: u8 = b'D';
    /// `X` - Order Cancel (12 B binary).
    pub const ORDER_CANCEL: u8 = b'X';
    /// `T` - Seconds timestamp marker (8 B binary).
    pub const SECONDS: u8 = b'T';
    /// `M` - Milliseconds timestamp marker (8 B binary).
    pub const MILLISECONDS: u8 = b'M';
    /// `S` - System Event (4 B binary).
    pub const SYSTEM_EVENT: u8 = b'S';
    /// `R` - Stock Directory (32 B binary).
    pub const STOCK_DIRECTORY: u8 = b'R';
    /// `r` - Extended Stock Directory (61 B binary).
    pub const EXTENDED_STOCK_DIRECTORY: u8 = b'r';
    /// `H` - Stock Trading Action (4 B binary).
    pub const STOCK_TRADING_ACTION: u8 = b'H';
    /// `P` - Trade (24 B binary).
    pub const TRADE: u8 = b'P';
    /// `Q` - Cross Trade (24 B binary).
    pub const CROSS_TRADE: u8 = b'Q';
    /// `B` - Broken Trade (8 B binary).
    pub const BROKEN_TRADE: u8 = b'B';
}

/// Binary message sizes in bytes (type char included) - the wire catalog's
/// `size` per `binary_messages` entry.
pub mod msg_len {
    /// Add Order (`A`) - 20 bytes.
    pub const ADD_ORDER: usize = 20;
    /// Order Executed (`E`) - 16 bytes.
    pub const ORDER_EXECUTED: usize = 16;
    /// Order Replaced (`U`) - 20 bytes.
    pub const ORDER_REPLACED: usize = 20;
    /// Order Delete (`D`) - 8 bytes.
    pub const ORDER_DELETE: usize = 8;
    /// Order Cancel (`X`) - 12 bytes.
    pub const ORDER_CANCEL: usize = 12;
    /// Seconds (`T`) - 8 bytes.
    pub const SECONDS: usize = 8;
    /// Milliseconds (`M`) - 8 bytes.
    pub const MILLISECONDS: usize = 8;
    /// System Event (`S`) - 4 bytes.
    pub const SYSTEM_EVENT: usize = 4;
    /// Stock Directory (`R`) - 32 bytes.
    pub const STOCK_DIRECTORY: usize = 32;
    /// Extended Stock Directory (`r`) - 61 bytes.
    pub const EXTENDED_STOCK_DIRECTORY: usize = 61;
    /// Stock Trading Action (`H`) - 4 bytes.
    pub const STOCK_TRADING_ACTION: usize = 4;
    /// Trade (`P`) - 24 bytes.
    pub const TRADE: usize = 24;
    /// Cross Trade (`Q`) - 24 bytes.
    pub const CROSS_TRADE: usize = 24;
    /// Broken Trade (`B`) - 8 bytes.
    pub const BROKEN_TRADE: usize = 8;
}

/// VER2 **text-mode** message sizes (the wire spec, the non-binary wire the
/// old TCP text feed used) - extracted from `Itch.cpp` `BuildItchMsg`
/// `snprintf` formats, NOT from the NASDAQ spec.
pub mod text_len {
    /// `ITCH_ADD_ORDER_MSG_LENGTH = 41` (`%1c%9u%1c%6u%-10s%10lu%3u%1c`).
    pub const ADD_ORDER: usize = 41;
    /// `ITCH_ADD_ORDER_MSG_LENGTH_LF = 45` (10-wide shares variant).
    pub const ADD_ORDER_LF: usize = 45;
    /// `ITCH_ORDER_EXECUTED_MSG_LENGTH = 29` (`%1c%9u%6u%9u%3u%1c`).
    pub const ORDER_EXECUTED: usize = 29;
    /// `ITCH_ORDER_EXECUTED_MSG_LENGTH_LF = 33`.
    pub const ORDER_EXECUTED_LF: usize = 33;
    /// `ITCH_TRADE_MSG_LENGTH = 52` (`%1c%9u%1c%6u%-10s%10lu%9u%3u%3u`).
    pub const TRADE: usize = 52;
    /// `ITCH_TRADE_MSG_LENGTH_LF = 56`.
    pub const TRADE_LF: usize = 56;
    /// `ITCH_ORDER_CANCEL_MSG_LENGTH = 16` (`%1c%9u%6u`).
    pub const ORDER_CANCEL: usize = 16;
    /// `ITCH_ORDER_CANCEL_MSG_LENGTH_LF = 20`.
    pub const ORDER_CANCEL_LF: usize = 20;
    /// `ITCH_ORDER_DELETE_MSG_LENGTH = 10` (`%1c%9u`).
    pub const ORDER_DELETE: usize = 10;
}

/// Largest share count the 6-wide text field holds (999 999); a size
/// beyond this switches five text families to their 10-wide variants.
pub const ITCH_MAX_SHARES_6: u32 = 999_999;

/// Largest price the 10-wide text field accommodates (9 999 999 999 in
/// venue-scale ticks); the VER2 text builders always use 10-wide prices.
pub const ITCH_MAX_PRICE_10: u64 = 9_999_999_999;

/// Typed failures of the decode-support surface.
///
/// # Examples
/// ```
/// use exchange8_itch::{ItchError, message_len};
///
/// let err = ItchError::UnknownType(b'Z');
/// assert_eq!(message_len(b'Z'), None);
/// # let _ = err; // Display impl exercised in the typed-error contract
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ItchError {
    /// A buffer ended before a field could be read.
    #[error("buffer too short: need {need}, have {have}")]
    TooShort {
        /// Bytes the operation needed.
        need: usize,
        /// Bytes actually available.
        have: usize,
    },
    /// A message type letter outside the VER2 catalog was encountered.
    #[error("unknown message type {0:#x}")]
    UnknownType(
        /// The unrecognized type byte.
        u8,
    ),
}

// ---------------------------------------------------------------------------
// Message structs (field order == wire order)
// ---------------------------------------------------------------------------

/// `A` Add Order (20 B binary).
///
/// Field order equals wire order: type char, side, `instrumentId` u16,
/// `orderId` u32, shares u32, price u32, broker u16, bookId char,
/// reserved char.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddOrder {
    /// `'B'`/`'A'` - the venue's `FEED_SIDE` letters.
    pub side: u8,
    /// Published instrument identifier (u16, big-endian).
    pub instrument_id: u16,
    /// Order identity as published and referenced by later events.
    pub order_id: u32,
    /// Share count on the visible order.
    pub shares: u32,
    /// Price in venue-scale ticks (echoed verbatim, no rescale here).
    pub price: u32,
    /// Broker code (already anonymized upstream per MD policy).
    pub broker: u16,
    /// Book letter ([`book_id`] values: dark pool, lit, QLX, MEMP).
    pub book_id: u8,
}

/// `E` Order Executed (16 B binary).
///
/// Reports a fill against a resting order id, with contra broker and the
/// platform's match number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderExecuted {
    /// Protocol marker byte (venue convention; `0x20` unless policy says otherwise).
    pub marker: u8,
    /// Contra-side broker code.
    pub contra_broker: u16,
    /// Resting order the execution applies to.
    pub order_id: u32,
    /// Executed share count.
    pub shares: u32,
    /// Platform match number tying this fill to trades/busts.
    pub match_number: u32,
}

/// `U` Order Replaced (20 B binary; 3 space-pad bytes precede the fields).
///
/// Carries the original and new order ids together with the new size/price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderReplaced {
    /// Order id being replaced.
    pub orig_order_id: u32,
    /// New order id taking over the resting interest.
    pub order_id: u32,
    /// New share count.
    pub shares: u32,
    /// New price in venue-scale ticks.
    pub price: u32,
}

/// `D` Order Delete (8 B binary; 3 space-pad bytes precede the field).
///
/// Removes the remainder of a resting order instantly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderDelete {
    /// Resting order to remove.
    pub order_id: u32,
}

/// `X` Order Cancel (12 B binary; 3 space-pad bytes precede the fields).
///
/// Trims a stated quantity from a resting order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderCancel {
    /// Resting order to trim.
    pub order_id: u32,
    /// Quantity removed by this cancel.
    pub shares: u32,
}

/// `P` Trade (24 B binary).
///
/// Prints a trade with both broker sides, book, and match number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trade {
    /// Book letter the trade printed on.
    pub book_id: u8,
    /// Published instrument identifier.
    pub instrument_id: u16,
    /// Originating (resting) order id.
    pub order_id: u32,
    /// Traded share count.
    pub shares: u32,
    /// Price in venue-scale ticks.
    pub price: u32,
    /// Platform match number.
    pub match_number: u32,
    /// Buy-side broker code.
    pub broker: u16,
    /// Sell-side broker code.
    pub contra_broker: u16,
}

/// `Q` Cross Trade (24 B binary, 2 trailing pad bytes).
///
/// Prints a cross (opening/closing/intermarket/various/midpoint semantics
/// are carried verbatim in [`CrossTrade::cross_type`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrossTrade {
    /// Cross-type letter, echoed verbatim from the trade.
    pub cross_type: u8,
    /// Published instrument identifier.
    pub instrument_id: u16,
    /// Crossed share count.
    pub shares: u32,
    /// Price in venue-scale ticks.
    pub price: u32,
    /// Platform match number.
    pub match_number: u32,
    /// Buy-side broker code.
    pub buy_broker: u16,
    /// Sell-side broker code.
    pub sell_broker: u16,
    /// Book letter the cross printed on.
    pub book_id: u8,
    /// Bypass flag letter (`'Y'` when the cross bypasses the book state).
    pub bypass: u8,
}

/// `B` Broken Trade (8 B binary; 3 space-pad bytes precede the field).
///
/// Busts a previously printed trade referenced by match number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenTrade {
    /// Match number of the trade being busted.
    pub match_number: u32,
}

/// `S` System Event (4 B binary, 2 trailing pad bytes).
///
/// Carries market-phase signal codes (start, startup, open, quoting, end,
/// post-end) verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemEvent {
    /// Phase-signal letter code.
    pub event_code: u8,
}

/// `H` Stock Trading Action (4 B binary).
///
/// Publishes a trading state (halted, paused, quoting) for an instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockTradingAction {
    /// Trading-state letter.
    pub trading_state: u8,
    /// Published instrument identifier.
    pub instrument_id: u16,
}

/// `T` Seconds (8 B) / `M` Milliseconds (8 B) timestamp marker.
///
/// One struct models both letters; [`Timestamp::encode`] takes the type
/// letter ([`msg_type::SECONDS`] or [`msg_type::MILLISECONDS`]) from the
/// caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    /// Seconds or milliseconds value (unit per the chosen type letter).
    pub value: u32,
}

// ---------------------------------------------------------------------------
// Encode (struct → bytes)
// ---------------------------------------------------------------------------

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}
fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}
fn push_pad3(out: &mut Vec<u8>) {
    out.extend_from_slice(&[ITCH_SPACE; 3]);
}

impl AddOrder {
    /// Encode as the 20-byte binary VER2 Add Order message.
    ///
    /// # Examples
    /// ```
    /// use exchange8_itch::{AddOrder, msg_len};
    ///
    /// let b = AddOrder {
    ///     side: b'B',
    ///     instrument_id: 7,
    ///     order_id: 42,
    ///     shares: 100,
    ///     price: 101_000,
    ///     broker: 3,
    ///     book_id: b'O',
    /// }
    /// .encode();
    /// assert_eq!(b.len(), msg_len::ADD_ORDER);
    /// assert_eq!(b[0], b'A');
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::ADD_ORDER);
        o.push(msg_type::ADD_ORDER);
        o.push(self.side);
        push_u16(&mut o, self.instrument_id);
        push_u32(&mut o, self.order_id);
        push_u32(&mut o, self.shares);
        push_u32(&mut o, self.price);
        push_u16(&mut o, self.broker);
        o.push(self.book_id);
        o.push(ITCH_SPACE);
        debug_assert_eq!(o.len(), msg_len::ADD_ORDER);
        o
    }
}

impl OrderExecuted {
    /// Encode as the 16-byte binary VER2 Order Executed message (byte 0 = `E`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::ORDER_EXECUTED);
        o.push(msg_type::ORDER_EXECUTED);
        o.push(self.marker);
        push_u16(&mut o, self.contra_broker);
        push_u32(&mut o, self.order_id);
        push_u32(&mut o, self.shares);
        push_u32(&mut o, self.match_number);
        debug_assert_eq!(o.len(), msg_len::ORDER_EXECUTED);
        o
    }
}

impl OrderReplaced {
    /// Encode as the 20-byte binary VER2 Order Replaced message (byte 0 = `U`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::ORDER_REPLACED);
        o.push(msg_type::ORDER_REPLACED);
        push_pad3(&mut o);
        push_u32(&mut o, self.orig_order_id);
        push_u32(&mut o, self.order_id);
        push_u32(&mut o, self.shares);
        push_u32(&mut o, self.price);
        debug_assert_eq!(o.len(), msg_len::ORDER_REPLACED);
        o
    }
}

impl OrderDelete {
    /// Encode as the 8-byte binary VER2 Order Delete message (byte 0 = `D`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::ORDER_DELETE);
        o.push(msg_type::ORDER_DELETE);
        push_pad3(&mut o);
        push_u32(&mut o, self.order_id);
        debug_assert_eq!(o.len(), msg_len::ORDER_DELETE);
        o
    }
}

impl OrderCancel {
    /// Encode as the 12-byte binary VER2 Order Cancel message (byte 0 = `X`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::ORDER_CANCEL);
        o.push(msg_type::ORDER_CANCEL);
        push_pad3(&mut o);
        push_u32(&mut o, self.order_id);
        push_u32(&mut o, self.shares);
        debug_assert_eq!(o.len(), msg_len::ORDER_CANCEL);
        o
    }
}

impl Trade {
    /// Encode as the 24-byte binary VER2 Trade message (byte 0 = `P`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::TRADE);
        o.push(msg_type::TRADE);
        o.push(self.book_id);
        push_u16(&mut o, self.instrument_id);
        push_u32(&mut o, self.order_id);
        push_u32(&mut o, self.shares);
        push_u32(&mut o, self.price);
        push_u32(&mut o, self.match_number);
        push_u16(&mut o, self.broker);
        push_u16(&mut o, self.contra_broker);
        debug_assert_eq!(o.len(), msg_len::TRADE);
        o
    }
}

impl CrossTrade {
    /// Encode as the 24-byte binary VER2 Cross Trade message (byte 0 = `Q`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::CROSS_TRADE);
        o.push(msg_type::CROSS_TRADE);
        o.push(self.cross_type);
        push_u16(&mut o, self.instrument_id);
        push_u32(&mut o, self.shares);
        push_u32(&mut o, self.price);
        push_u32(&mut o, self.match_number);
        push_u16(&mut o, self.buy_broker);
        push_u16(&mut o, self.sell_broker);
        o.push(self.book_id);
        o.push(self.bypass);
        o.push(ITCH_SPACE);
        o.push(ITCH_SPACE);
        debug_assert_eq!(o.len(), msg_len::CROSS_TRADE);
        o
    }
}

impl BrokenTrade {
    /// Encode as the 8-byte binary VER2 Broken Trade message (byte 0 = `B`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::BROKEN_TRADE);
        o.push(msg_type::BROKEN_TRADE);
        push_pad3(&mut o);
        push_u32(&mut o, self.match_number);
        debug_assert_eq!(o.len(), msg_len::BROKEN_TRADE);
        o
    }
}

impl SystemEvent {
    /// Encode as the 4-byte binary VER2 System Event message (byte 0 = `S`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::SYSTEM_EVENT);
        o.push(msg_type::SYSTEM_EVENT);
        o.push(self.event_code);
        o.push(ITCH_SPACE);
        o.push(ITCH_SPACE);
        debug_assert_eq!(o.len(), msg_len::SYSTEM_EVENT);
        o
    }
}

impl StockTradingAction {
    /// Encode as the 4-byte binary VER2 Stock Trading Action message
    /// (byte 0 = `H`).
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::STOCK_TRADING_ACTION);
        o.push(msg_type::STOCK_TRADING_ACTION);
        o.push(self.trading_state);
        push_u16(&mut o, self.instrument_id);
        debug_assert_eq!(o.len(), msg_len::STOCK_TRADING_ACTION);
        o
    }
}

impl Timestamp {
    /// Encode as the 8-byte binary VER2 timestamp message, with `kind`
    /// ([`msg_type::SECONDS`] or [`msg_type::MILLISECONDS`]) as byte 0.
    ///
    /// # Examples
    /// ```
    /// use exchange8_itch::{Timestamp, msg_type, msg_len};
    ///
    /// let b = Timestamp { value: 123 }.encode(msg_type::SECONDS);
    /// assert_eq!(b.len(), msg_len::SECONDS);
    /// assert_eq!(b[0], b'T');
    /// ```
    pub fn encode(&self, kind: u8) -> Vec<u8> {
        let mut o = Vec::with_capacity(msg_len::SECONDS);
        o.push(kind);
        push_pad3(&mut o);
        push_u32(&mut o, self.value);
        debug_assert_eq!(o.len(), msg_len::SECONDS);
        o
    }
}

// ---------------------------------------------------------------------------
// VER2 text-mode builders (the platform spec `BuildItchMsg` snprintf parity)
// ---------------------------------------------------------------------------

/// Right-pad a numeric field to `width` characters (right-aligned digits,
/// space-filled - `snprintf`'s `%Nu` pads with spaces, not zeros).
fn pad_num(v: u64, width: usize) -> String {
    let s = v.to_string();
    if s.len() >= width {
        s
    } else {
        format!("{}{}", " ".repeat(width - s.len()), s)
    }
}

/// Left-aligned string clamped to `width` (`%-10s` semantics).
fn pad_str(s: &str, width: usize) -> String {
    if s.len() >= width {
        s[..width].to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - s.len()))
    }
}

impl AddOrder {
    /// Encode as the fixed-width text-mode Add Order form (41 B narrow,
    /// `%1c%9u%1c%6u%-10s%10lu%3u%1c`); switches to the 45-byte 10-wide
    /// share field when shares exceed [`ITCH_MAX_SHARES_6`].
    ///
    /// # Examples
    /// ```
    /// use exchange8_itch::{AddOrder, text_len};
    ///
    /// let t = AddOrder {
    ///     side: b'B',
    ///     instrument_id: 7,
    ///     order_id: 42,
    ///     shares: 100,
    ///     price: 1_005,
    ///     broker: 9,
    ///     book_id: 1,
    /// }
    /// .encode_text();
    /// assert_eq!(t.len(), text_len::ADD_ORDER);
    /// assert_eq!(&t[1..10], b"       42");
    /// ```
    pub fn encode_text(&self) -> Vec<u8> {
        let wide = self.shares > ITCH_MAX_SHARES_6;
        let shares_width = if wide { 10 } else { 6 };
        let total = if wide {
            text_len::ADD_ORDER_LF
        } else {
            text_len::ADD_ORDER
        };
        let mut o = String::with_capacity(total);
        o.push(msg_type::ADD_ORDER as char);
        o.push_str(&pad_num(self.order_id as u64, 9));
        o.push(self.side as char);
        o.push_str(&pad_num(self.shares as u64, shares_width));
        o.push_str(&pad_str(&format!("instr{}", self.instrument_id), 10));
        o.push_str(&pad_num(self.price as u64, 10));
        o.push_str(&pad_num(self.broker as u64, 3));
        o.push(ITCH_SPACE as char);
        debug_assert_eq!(o.len(), total);
        o.into_bytes()
    }
}

impl OrderExecuted {
    /// Encode as the fixed-width text-mode Order Executed form (29 B narrow,
    /// `%1c%9u%6u%9u%3u%1c`); switches to the 33-byte 10-wide share field
    /// when shares exceed [`ITCH_MAX_SHARES_6`].
    pub fn encode_text(&self) -> Vec<u8> {
        let wide = self.shares > ITCH_MAX_SHARES_6;
        let shares_width = if wide { 10 } else { 6 };
        let total = if wide {
            text_len::ORDER_EXECUTED_LF
        } else {
            text_len::ORDER_EXECUTED
        };
        let mut o = String::with_capacity(total);
        o.push(msg_type::ORDER_EXECUTED as char);
        o.push_str(&pad_num(self.order_id as u64, 9));
        o.push_str(&pad_num(self.shares as u64, shares_width));
        o.push_str(&pad_num(self.match_number as u64, 9));
        o.push_str(&pad_num(self.contra_broker as u64, 3));
        o.push(self.marker as char);
        debug_assert_eq!(o.len(), total);
        o.into_bytes()
    }
}

impl Trade {
    /// Encode as the fixed-width text-mode Trade form (52 B narrow,
    /// `%1c%9u%1c%6u%-10s%10lu%9u%3u%3u`); switches to the 56-byte 10-wide
    /// share field when shares exceed [`ITCH_MAX_SHARES_6`].
    ///
    /// The side slot after the order id carries the executed order's side;
    /// the struct has no side field, so the encoder pins the byte to `'B'`
    /// (buy-side printing convention, pinned by the `text_trade_layout`
    /// golden test). Carry the side on the struct before changing it.
    pub fn encode_text(&self) -> Vec<u8> {
        let wide = self.shares > ITCH_MAX_SHARES_6;
        let shares_width = if wide { 10 } else { 6 };
        let total = if wide {
            text_len::TRADE_LF
        } else {
            text_len::TRADE
        };
        let mut o = String::with_capacity(total);
        o.push(msg_type::TRADE as char);
        o.push_str(&pad_num(self.order_id as u64, 9));
        o.push(b'B' as char);
        o.push_str(&pad_num(self.shares as u64, shares_width));
        o.push_str(&pad_str(&format!("instr{}", self.instrument_id), 10));
        o.push_str(&pad_num(self.price as u64, 10));
        o.push_str(&pad_num(self.match_number as u64, 9));
        o.push_str(&pad_num(self.broker as u64, 3));
        o.push_str(&pad_num(self.contra_broker as u64, 3));
        debug_assert_eq!(o.len(), total);
        o.into_bytes()
    }
}

impl OrderCancel {
    /// Encode as the fixed-width text-mode Order Cancel form (16 B narrow,
    /// `%1c%9u%6u`); switches to the 20-byte 10-wide share field when
    /// cancelled shares exceed [`ITCH_MAX_SHARES_6`].
    pub fn encode_text(&self) -> Vec<u8> {
        let wide = self.shares > ITCH_MAX_SHARES_6;
        let shares_width = if wide { 10 } else { 6 };
        let total = if wide {
            text_len::ORDER_CANCEL_LF
        } else {
            text_len::ORDER_CANCEL
        };
        let mut o = String::with_capacity(total);
        o.push(msg_type::ORDER_CANCEL as char);
        o.push_str(&pad_num(self.order_id as u64, 9));
        o.push_str(&pad_num(self.shares as u64, shares_width));
        debug_assert_eq!(o.len(), total);
        o.into_bytes()
    }
}

impl OrderDelete {
    /// Encode as the fixed-width text-mode Order Delete form (10 B,
    /// `%1c%9u` - no wide variant; the id field is fixed 9-wide).
    pub fn encode_text(&self) -> Vec<u8> {
        let mut o = String::with_capacity(text_len::ORDER_DELETE);
        o.push(msg_type::ORDER_DELETE as char);
        o.push_str(&pad_num(self.order_id as u64, 9));
        debug_assert_eq!(o.len(), text_len::ORDER_DELETE);
        o.into_bytes()
    }
}

// ---------------------------------------------------------------------------
// Decode helpers (bytes → fields) - used by golden tests + consumers
// ---------------------------------------------------------------------------

/// Read a big-endian `u16` at byte offset `off`.
///
/// Returns `None` when the buffer ends before the field (`# Errors` are
/// reported by value, never by panic).
///
/// # Examples
/// ```
/// use exchange8_itch::read_u16;
///
/// assert_eq!(read_u16(&[0x00, 0x07], 0), Some(7));
/// assert_eq!(read_u16(&[0x00], 0), None);
/// ```
pub fn read_u16(buf: &[u8], off: usize) -> Option<u16> {
    let b = buf.get(off..off + 2)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

/// Read a big-endian `u32` at byte offset `off`.
///
/// Returns `None` when the buffer ends before the field.
///
/// # Examples
/// ```
/// use exchange8_itch::read_u32;
///
/// assert_eq!(read_u32(&[0x00, 0x00, 0x00, 0x2A], 0), Some(42));
/// assert_eq!(read_u32(&[0x00, 0x00], 0), None);
/// ```
pub fn read_u32(buf: &[u8], off: usize) -> Option<u32> {
    let b = buf.get(off..off + 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Expected binary message length for a VER2 type letter, type char
/// included; `None` for letters outside the catalog.
///
/// # Examples
/// ```
/// use exchange8_itch::message_len;
///
/// assert_eq!(message_len(b'A'), Some(20));
/// assert_eq!(message_len(b'S'), Some(4));
/// assert_eq!(message_len(b'Z'), None);
/// ```
pub fn message_len(type_char: u8) -> Option<usize> {
    Some(match type_char {
        msg_type::ADD_ORDER => msg_len::ADD_ORDER,
        msg_type::ORDER_EXECUTED => msg_len::ORDER_EXECUTED,
        msg_type::ORDER_REPLACED => msg_len::ORDER_REPLACED,
        msg_type::ORDER_DELETE => msg_len::ORDER_DELETE,
        msg_type::ORDER_CANCEL => msg_len::ORDER_CANCEL,
        msg_type::SECONDS | msg_type::MILLISECONDS => msg_len::SECONDS,
        msg_type::SYSTEM_EVENT => msg_len::SYSTEM_EVENT,
        msg_type::STOCK_DIRECTORY => msg_len::STOCK_DIRECTORY,
        msg_type::EXTENDED_STOCK_DIRECTORY => msg_len::EXTENDED_STOCK_DIRECTORY,
        msg_type::STOCK_TRADING_ACTION => msg_len::STOCK_TRADING_ACTION,
        msg_type::TRADE => msg_len::TRADE,
        msg_type::CROSS_TRADE => msg_len::CROSS_TRADE,
        msg_type::BROKEN_TRADE => msg_len::BROKEN_TRADE,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden: Add Order byte layout - anchored to `wire-formats.json`
    /// `binary_messages[0]`: type,side,instrumentId u16,orderId u32,shares u32,
    /// price u32,broker u16,bookId,reserved = 20 B.
    #[test]
    fn add_order_golden_bytes() {
        let m = AddOrder {
            side: b'B',
            instrument_id: 7,
            order_id: 42,
            shares: 100,
            price: 101_000,
            broker: 3,
            book_id: book_id::MEMP,
        };
        let b = m.encode();
        assert_eq!(b.len(), 20);
        assert_eq!(b[0], b'A');
        assert_eq!(b[1], b'B');
        assert_eq!(read_u16(&b, 2), Some(7));
        assert_eq!(read_u32(&b, 4), Some(42));
        assert_eq!(read_u32(&b, 8), Some(100));
        assert_eq!(read_u32(&b, 12), Some(101_000));
        assert_eq!(read_u16(&b, 16), Some(3));
        assert_eq!(b[18], b'O');
        assert_eq!(b[19], ITCH_SPACE);
        // Spot-check big-endian: instrument 7 → 0x00 0x07
        assert_eq!(&b[2..4], &[0x00, 0x07]);
    }

    #[test]
    fn order_executed_golden_bytes() {
        let m = OrderExecuted {
            marker: 0x20,
            contra_broker: 5,
            order_id: 9,
            shares: 50,
            match_number: 777,
        };
        let b = m.encode();
        assert_eq!(b.len(), 16);
        assert_eq!(b[0], b'E');
        assert_eq!(b[1], 0x20);
        assert_eq!(read_u16(&b, 2), Some(5));
        assert_eq!(read_u32(&b, 4), Some(9));
        assert_eq!(read_u32(&b, 8), Some(50));
        assert_eq!(read_u32(&b, 12), Some(777));
    }

    #[test]
    fn trade_and_cross_golden_bytes() {
        let t = Trade {
            book_id: b'O',
            instrument_id: 7,
            order_id: 1,
            shares: 100,
            price: 101_000,
            match_number: 55,
            broker: 2,
            contra_broker: 3,
        };
        assert_eq!(t.encode().len(), 24);
        let c = CrossTrade {
            cross_type: b'I',
            instrument_id: 7,
            shares: 500,
            price: 101_000,
            match_number: 56,
            buy_broker: 2,
            sell_broker: 3,
            book_id: b'O',
            bypass: b'Y',
        };
        assert_eq!(c.encode().len(), 24);
    }

    #[test]
    fn all_sizes_match_catalog() {
        // Every encoder must produce exactly the catalog length.
        assert_eq!(OrderDelete { order_id: 1 }.encode().len(), 8);
        assert_eq!(
            OrderCancel {
                order_id: 1,
                shares: 5
            }
            .encode()
            .len(),
            12
        );
        assert_eq!(BrokenTrade { match_number: 9 }.encode().len(), 8);
        assert_eq!(SystemEvent { event_code: b'Q' }.encode().len(), 4);
        assert_eq!(
            StockTradingAction {
                trading_state: b'H',
                instrument_id: 1
            }
            .encode()
            .len(),
            4
        );
        assert_eq!(Timestamp { value: 123 }.encode(msg_type::SECONDS).len(), 8);
        assert_eq!(
            OrderReplaced {
                orig_order_id: 1,
                order_id: 2,
                shares: 3,
                price: 4
            }
            .encode()
            .len(),
            20
        );
    }

    #[test]
    fn message_len_table() {
        assert_eq!(message_len(b'A'), Some(20));
        assert_eq!(message_len(b'Q'), Some(24));
        assert_eq!(message_len(b'Z'), None);
    }

    // ------------------------------------------------------------------
    // VER2 text-mode builders - golden against `Itch.h` lengths + the
    // `Itch.cpp` snprintf format strings.
    // ------------------------------------------------------------------

    #[test]
    fn text_add_order_layout() {
        let m = AddOrder {
            side: b'B',
            instrument_id: 7,
            order_id: 42,
            shares: 100,
            price: 10_05,
            broker: 9,
            book_id: 1,
        };
        let t = m.encode_text();
        assert_eq!(t.len(), text_len::ADD_ORDER);
        // Fields: type, ref(9), side, shares(6), symbol(10), price(10),
        // broker(3), reserved(1).
        assert_eq!(t[0], b'A');
        assert_eq!(&t[1..10], b"       42"); // ref right-aligned
        assert_eq!(t[10], b'B');
        assert_eq!(&t[11..17], b"   100"); // shares 6-wide
        assert_eq!(&t[17..27], b"instr7    "); // symbol left-aligned, 10 B
        assert_eq!(&t[27..37], b"      1005"); // price 10-wide: 6 spaces + 1005
        assert_eq!(&t[37..40], b"  9"); // broker 3-wide
        assert_eq!(t[40], b' ');
    }

    #[test]
    fn text_add_order_wide_shares_variant() {
        let m = AddOrder {
            side: b'A',
            instrument_id: 3,
            order_id: 1,
            shares: 1_500_000,    // > 999_999 → 10-wide, 45 B
            price: 4_294_967_295, // u32::MAX - max 10-wide price
            broker: 12,
            book_id: 2,
        };
        let t = m.encode_text();
        assert_eq!(t.len(), text_len::ADD_ORDER_LF);
        assert_eq!(&t[11..21], b"   1500000"); // 10-wide shares
    }

    #[test]
    fn text_order_executed_layout() {
        let m = OrderExecuted {
            marker: b'X',
            contra_broker: 77,
            order_id: 42,
            shares: 500,
            match_number: 9001,
        };
        let t = m.encode_text();
        assert_eq!(t.len(), text_len::ORDER_EXECUTED);
        assert_eq!(t[0], b'E');
        assert_eq!(&t[1..10], b"       42");
        assert_eq!(&t[10..16], b"   500");
        assert_eq!(&t[16..25], b"     9001");
        assert_eq!(&t[25..28], b" 77");
        assert_eq!(t[28], b'X');
    }

    #[test]
    fn text_order_executed_wide_variant() {
        let m = OrderExecuted {
            marker: b'M',
            contra_broker: 5,
            order_id: 1,
            shares: 2_000_000,
            match_number: 2,
        };
        assert_eq!(m.encode_text().len(), text_len::ORDER_EXECUTED_LF);
    }

    #[test]
    fn text_trade_layout() {
        let m = Trade {
            book_id: 1,
            instrument_id: 7,
            order_id: 42,
            shares: 100,
            price: 1_005,
            match_number: 9001,
            broker: 101,
            contra_broker: 202,
        };
        let t = m.encode_text();
        assert_eq!(t.len(), text_len::TRADE);
        // type(1) ref(9) side(1) shares(6) symbol(10) price(10) match(9)
        // buyBroker(3) sellBroker(3) = 52.
        assert_eq!(t[0], b'P');
        assert_eq!(&t[1..10], b"       42");
        assert_eq!(t[10], b'B');
        assert_eq!(&t[11..17], b"   100");
        assert_eq!(&t[17..27], b"instr7    ");
        assert_eq!(&t[27..37], b"      1005");
        assert_eq!(&t[37..46], b"     9001");
        assert_eq!(&t[46..49], b"101");
        assert_eq!(&t[49..52], b"202");
    }

    #[test]
    fn text_order_cancel_layout() {
        let m = OrderCancel {
            order_id: 42,
            shares: 100,
        };
        let t = m.encode_text();
        assert_eq!(t.len(), text_len::ORDER_CANCEL);
        assert_eq!(t[0], b'X');
        assert_eq!(&t[1..10], b"       42");
        assert_eq!(&t[10..16], b"   100");
        // Wide variant.
        let w = OrderCancel {
            order_id: 1,
            shares: 1_000_000,
        };
        assert_eq!(w.encode_text().len(), text_len::ORDER_CANCEL_LF);
    }

    #[test]
    fn text_order_delete_layout() {
        let m = OrderDelete { order_id: 42 };
        let t = m.encode_text();
        assert_eq!(t.len(), text_len::ORDER_DELETE);
        assert_eq!(t[0], b'D');
        assert_eq!(&t[1..10], b"       42");
    }

    #[test]
    fn text_length_constants_match_spec() {
        // Pinned to Itch.h VER2 block.
        assert_eq!(text_len::ADD_ORDER, 41);
        assert_eq!(text_len::ADD_ORDER_LF, 45);
        assert_eq!(text_len::ORDER_EXECUTED, 29);
        assert_eq!(text_len::ORDER_EXECUTED_LF, 33);
        assert_eq!(text_len::TRADE, 52);
        assert_eq!(text_len::TRADE_LF, 56);
        assert_eq!(text_len::ORDER_CANCEL, 16);
        assert_eq!(text_len::ORDER_CANCEL_LF, 20);
        assert_eq!(text_len::ORDER_DELETE, 10);
        assert_eq!(ITCH_MAX_SHARES_6, 999_999);
    }
}
