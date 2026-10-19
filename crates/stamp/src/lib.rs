// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! # exchange8::stamp - STAMP 4.0 wire codec
//!
//! The STAMP 4.0 wire-format codec of the exchange8 vertical: decoding and
//! encoding of TSX venue-feed/order frames - control header, business
//! section, and control trailer - with lossless field order and indexed
//! (`tag.index`) field preservation. The source of truth for every wire
//! decision (framing bytes, tags, business classes) is this crate; the
//! detailed design lives in `crates/stamp/DESIGN.md` (§2 module map, §4 API
//! surface), the overview in `crates/stamp/README.md`.
//!
//! # Wire format
//!
//! ```text
//! <SOH 0x01> control-header <FS 0x1C> business <GS 0x1D> control-trailer
//! ```
//! - Each section is a sequence of fields separated by `RS 0x1E`.
//! - Each field is `<tag>[.<index>]=<value>`; the `.index` suffix is
//!   written only for non-zero indices.
//! - Control header and trailer are mandatory framing; the trailer may be
//!   absent in truncated captures (the parser tolerates).
//!
//! # Quick-start
//!
//! ```no_run
//! use exchange8_stamp::{StampMessage, tag, business_class};
//!
//! // Decode a frame received from the feed session.
//! let frame: Vec<u8> = /* frame bytes from the session layer */ vec![];
//! match StampMessage::parse(&frame) {
//!     Ok(msg) => {
//!         if msg.business_class() == Some(business_class::TRADE_CONFIRM) {
//!             let px = msg.business.get(tag::LAST_FILL_PRICE);
//!             let qty = msg.business.get(tag::FILL_VOLUME);
//!             // values are verbatim text; arithmetic belongs downstream
//!         }
//!     }
//!     Err(err) => {
//!         // typed failure: skip the frame / feed-status event upstream
//!     }
//! }
//! ```
//!
//! Consumers own transport and session supervision (see `DESIGN.md` §1 for
//! the exact boundary).

use thiserror::Error;

/// Control-header separator byte opening every STAMP frame.
///
/// The first byte of any decodable frame must equal this constant;
/// anything else is [`StampError::NoControlHeader`].
///
/// # Examples
/// ```
/// # use exchange8_stamp::{CONTROL_HEADER_CHAR, StampMessage};
/// let mut frame = vec![CONTROL_HEADER_CHAR];
/// frame.extend_from_slice(b"23=30");
/// assert!(StampMessage::parse(&frame).is_ok());
/// ```
pub const CONTROL_HEADER_CHAR: u8 = 0x01;
/// Business-section separator byte between control header and business.
pub const BUSINESS_CONTENT_CHAR: u8 = 0x1c;
/// Control-trailer separator byte closing the frame (optional on truncated
/// captures).
pub const CONTROL_TRAILER_CHAR: u8 = 0x1d;
/// Field-separator byte between fields inside one section.
pub const FIELD_CHAR: u8 = 0x1e;
/// Separator between tag number and field index in a field identifier
/// (`tag.index`).
pub const DOT_CHAR: u8 = b'.';
/// Separator between field identifier and field value.
pub const EQUAL_CHAR: u8 = b'=';

/// STAMP protocol version string carried in the control header (`tag 65`).
///
/// # Examples
/// ```
/// # use exchange8_stamp::STAMP_VERSION_NUMBER;
/// assert_eq!(STAMP_VERSION_NUMBER, "Version 4.0");
/// ```
pub const STAMP_VERSION_NUMBER: &str = "Version 4.0";

/// STAMP wire tags - the subset the exchange consumes.
///
/// Numbers are the venue's wire numbering; adapters and tests address
/// fields through these constants rather than local magic integers. An
/// unknown tag still parses losslessly (opaque value); this module pins
/// the names the exchange addresses directly.
///
/// # Examples
/// ```
/// # use exchange8_stamp::tag;
/// assert_eq!(tag::SYMBOL, 55);
/// assert_eq!(tag::PRICE, 41);
/// ```
pub mod tag {
    /// Account identifier field.
    pub const ACCOUNT_ID: i32 = 1;
    /// Account classification field.
    pub const ACCOUNT_TYPE: i32 = 2;
    /// Control-message class discriminator (control header/trailer).
    pub const CONTROL_CLASS: i32 = 3;
    /// Business action qualifier.
    pub const BUSINESS_ACTION: i32 = 5;
    /// Business-message class discriminator (the message type).
    pub const BUSINESS_CLASS: i32 = 6;
    /// Buyer-side order number on a trade confirmation.
    pub const BUY_ORDER_NUMBER: i32 = 10;
    /// Session heartbeat interval.
    pub const HEARTBEAT_PERIOD: i32 = 23;
    /// Top of the acknowledged sequence window.
    pub const HIGH_SEQUENCE_NUMBER: i32 = 24;
    /// Bottom of the acknowledged sequence window.
    pub const LOW_SEQUENCE_NUMBER: i32 = 27;
    /// New-order sequence reference.
    pub const NEW_ORDER_NUMBER: i32 = 32;
    /// Next expected sequence number.
    pub const NEXT_SEQUENCE_NUMBER: i32 = 33;
    /// Order-book identifier.
    pub const ORDER_BOOK: i32 = 38;
    /// Order duration (time-in-force) code.
    pub const ORDER_DURATION: i32 = 39;
    /// Order reference number.
    pub const ORDER_NUMBER: i32 = 40;
    /// Price field (venue-scaled text; no scale is applied here).
    pub const PRICE: i32 = 41;
    /// Sequence number (may repeat as an indexed multi-value field).
    pub const SEQUENCE_NUMBER: i32 = 50;
    /// Settlement terms code.
    pub const SETTLEMENT_TERMS: i32 = 53;
    /// Instrument symbol.
    pub const SYMBOL: i32 = 55;
    /// Message timestamp.
    pub const TIMESTAMP: i32 = 56;
    /// Currency code.
    pub const CURRENCY: i32 = 58;
    /// User identifier.
    pub const USER_ID: i32 = 62;
    /// Volume field (venue-scaled text).
    pub const VOLUME: i32 = 64;
    /// Protocol version string.
    pub const VERSION_NUMBER: i32 = 65;
    /// Broker (member firm) number.
    pub const BROKER_NUMBER: i32 = 70;
    /// Principal-trade flag.
    pub const PRINCIPAL_TRADE: i32 = 71;
    /// Remaining (open) volume on a working order.
    pub const REMAINING_VOLUME: i32 = 72;
    /// Crossed sell volume on a cross trade.
    pub const CROSS_SELL_VOLUME: i32 = 73;
    /// Filled volume on an execution.
    pub const FILL_VOLUME: i32 = 77;
    /// Cancel rejection code.
    pub const CANCEL_CODE: i32 = 78;
    /// Trading state of the instrument.
    pub const STOCK_STATE: i32 = 81;
    /// Instrument board/group assignment.
    pub const STOCK_GROUP: i32 = 82;
    /// Buy-side broker number on a trade.
    pub const BUY_BROKER_NUMBER: i32 = 83;
    /// Sell-side broker number on a trade.
    pub const SELL_BROKER_NUMBER: i32 = 84;
    /// Cash-order flag.
    pub const CASH_ORDER: i32 = 85;
    /// Cash (venue) trade identifier.
    pub const CASH_TRADE_ID: i32 = 86;
    /// Last fill price on an execution.
    pub const LAST_FILL_PRICE: i32 = 87;
    /// Workable order state.
    pub const ORDER_STATE: i32 = 88;
    /// Participant identifier.
    pub const PARTICIPANT_ID: i32 = 89;
    /// Stop price for stop orders.
    pub const STOP_PRICE: i32 = 90;
    /// Refreshed (iceberg-reload style) volume.
    pub const REFRESH_VOLUME: i32 = 94;
    /// Original (clearing) firm identifier.
    pub const ORIGINAL_FIRM: i32 = 100;
    /// Entering (originating) firm identifier.
    pub const ENTERING_FIRM: i32 = 101;
    /// Shares traded on the last fill.
    pub const LAST_SHARES_TRADED: i32 = 102;
    /// Market-on-close eligibility flag.
    pub const MOC_ELIGIBLE: i32 = 103;
    /// Order side code.
    pub const ORDER_SIDE: i32 = 104;
    /// Cross-trade type code.
    pub const CROSS_TYPE: i32 = 106;
    /// Order type code.
    pub const ORDER_TYPE: i32 = 107;
    /// Participant classification.
    pub const PARTICIPANT_TYPE: i32 = 110;
    /// Time of the last trade on the instrument.
    pub const LAST_TRADE_TIME: i32 = 112;
    /// Vendor reference number.
    pub const REFERENCE_NUMBER: i32 = 113;
    /// Buy-plus-sell volume aggregate.
    pub const BUY_SELL_VOLUME: i32 = 114;
    /// Instrument volume-weighted average price.
    pub const STOCK_VWAP: i32 = 115;
    /// Instrument opening price.
    pub const STOCK_OPEN_PRICE: i32 = 116;
    /// Instrument high price.
    pub const STOCK_HIGH_PRICE: i32 = 117;
    /// Instrument low price.
    pub const STOCK_LOW_PRICE: i32 = 118;
    /// Instrument closing price.
    pub const STOCK_CLOSE_PRICE: i32 = 119;
    /// Instrument aggregate volume.
    pub const STOCK_VOLUME: i32 = 120;
    /// Instrument trade count.
    pub const STOCK_TRADE_COUNT: i32 = 121;
    /// Instrument market-wide VWAP.
    pub const STOCK_MARKET_VWAP: i32 = 122;
    /// Side code on quote/trade payloads.
    pub const SIDE: i32 = 129;
    /// Scheduled resume time after a halt.
    pub const STOCK_RESUME_TIME: i32 = 132;
}

/// Business-class codes for order/trade messages.
///
/// The class discriminator rides the business section as `tag 6` and
/// classifies the payload; [`StampMessage::business_class`] reads it back.
///
/// # Examples
/// ```
/// # use exchange8_stamp::{StampMessage, business_class};
/// let msg = StampMessage::build(business_class::ORDER_ACK, vec![]);
/// assert_eq!(msg.business_class(), Some(business_class::ORDER_ACK));
/// ```
pub mod business_class {
    /// Order entry request.
    pub const ORDER_REQUEST: &str = "001";
    /// Quote request.
    pub const QUOTE_REQUEST: &str = "002";
    /// Instrument status request.
    pub const STOCK_STATUS_REQUEST: &str = "003";
    /// Order cancel request.
    pub const ORDER_CANCEL_REQUEST: &str = "005";
    /// Order cancel confirmation.
    pub const ORDER_CANCEL_CONFIRM: &str = "006";
    /// Order cancel rejection.
    pub const ORDER_CANCEL_REJECT: &str = "007";
    /// Trade confirmation (execution report).
    pub const TRADE_CONFIRM: &str = "008";
    /// Business-level rejection.
    pub const BUSINESS_REJECT: &str = "009";
    /// Order acknowledgement.
    pub const ORDER_ACK: &str = "010";
}

/// Parse/encode failures - the crate's entire error surface.
///
/// Every grammar violation the codec can detect is one of these three
/// typed errors; the codec never repairs malformed input silently.
///
/// # Examples
/// ```
/// # use exchange8_stamp::{StampError, StampMessage};
/// // Not starting with the control-header byte.
/// assert_eq!(StampMessage::parse(b"50=1"), Err(StampError::NoControlHeader));
/// assert!(StampError::NoControlHeader.to_string().contains("control header"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StampError {
    /// The buffer is empty or its first byte is not the control-header
    /// separator (`0x01`); nothing can be parsed.
    #[error("message does not start with the control header (0x01)")]
    NoControlHeader,
    /// A field is missing its `=` identifier/value separator (including
    /// non-numeric tag or index identifiers).
    #[error("field has no '=' separator")]
    MissingEquals,
    /// Field bytes are not valid UTF-8.
    #[error("non-utf8 bytes in field")]
    InvalidUtf8,
}

/// One parsed field: `(tag, index, value)` - indexed fields keep their index.
///
/// The wire identity of a field is `tag` when `index` is `0` and
/// `tag.index` otherwise; the dot suffix is written on encode only for
/// non-zero indices, so wire text and parsed identity are one-to-one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StampField {
    /// The venue's numbered tag.
    pub tag: i32,
    /// Occurrence index (`0` for unindexed fields).
    pub index: i32,
    /// Verbatim UTF-8 field value; never interpreted by the codec.
    pub value: String,
}

/// One section (control header / business / control trailer): an ordered
/// list of [`StampField`]s.
///
/// Field order is stored order. Lookup accessors preserve the wire-mirror
/// semantics: [`StampSection::get`] returns the first index-0 value,
/// [`StampSection::get_indexed`] matches an exact index, and
/// [`StampSection::get_all`] returns every value for a tag in stored order.
/// # Examples
/// ```
/// # use exchange8_stamp::{StampSection, StampMessage, tag};
/// let parsed = StampMessage::parse(b"\x01\x1c55=ABC\x1e55.1=XYZ\x1d").unwrap();
/// assert_eq!(parsed.business.get(tag::SYMBOL), Some("ABC"));
/// assert_eq!(parsed.business.get_indexed(tag::SYMBOL, 1), Some("XYZ"));
/// assert_eq!(parsed.business.get_all(tag::SYMBOL), vec!["ABC", "XYZ"]);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StampSection {
    /// Fields in stored (wire) order.
    pub fields: Vec<StampField>,
}

impl StampSection {
    fn parse(buf: &[u8]) -> Result<Self, StampError> {
        let mut fields = Vec::new();
        // Sections may be empty (just the delimiter).
        if buf.is_empty() {
            return Ok(StampSection { fields });
        }
        let mut rest = buf;
        loop {
            let end = rest.iter().position(|&b| b == FIELD_CHAR);
            let field_bytes = match end {
                Some(p) => &rest[..p],
                None => rest,
            };
            if !field_bytes.is_empty() {
                fields.push(parse_field(field_bytes)?);
            }
            match end {
                Some(p) => rest = &rest[p + 1..],
                None => break,
            }
        }
        Ok(StampSection { fields })
    }

    /// First value for `tag` with index 0.
    ///
    /// # Examples
    /// ```
    /// # use exchange8_stamp::{StampMessage, tag};
    /// let msg = StampMessage::parse(b"\x01\x1c55=ABC").unwrap();
    /// assert_eq!(msg.business.get(tag::SYMBOL), Some("ABC"));
    /// assert_eq!(msg.business.get(tag::PRICE), None);
    /// ```
    pub fn get(&self, tag: i32) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.tag == tag && f.index == 0)
            .map(|f| f.value.as_str())
    }

    /// Indexed value (`tag.1`, `tag.2`, …).
    ///
    /// # Examples
    /// ```
    /// # use exchange8_stamp::{StampField, StampMessage, tag};
    /// let mut msg = StampMessage::default();
    /// msg.business.fields = vec![
    ///     StampField { tag: tag::SEQUENCE_NUMBER, index: 1, value: "a".into() },
    ///     StampField { tag: tag::SEQUENCE_NUMBER, index: 2, value: "b".into() },
    /// ];
    /// assert_eq!(msg.business.get_indexed(tag::SEQUENCE_NUMBER, 2), Some("b"));
    /// ```
    pub fn get_indexed(&self, tag: i32, index: i32) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.tag == tag && f.index == index)
            .map(|f| f.value.as_str())
    }

    /// All values for `tag` across indices, in stored order.
    ///
    /// # Examples
    /// ```
    /// # use exchange8_stamp::{StampField, StampMessage, tag};
    /// let mut msg = StampMessage::default();
    /// msg.business.fields = vec![
    ///     StampField { tag: tag::SEQUENCE_NUMBER, index: 1, value: "a".into() },
    ///     StampField { tag: tag::SEQUENCE_NUMBER, index: 2, value: "b".into() },
    /// ];
    /// assert_eq!(msg.business.get_all(tag::SEQUENCE_NUMBER), vec!["a", "b"]);
    /// ```
    pub fn get_all(&self, tag: i32) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|f| f.tag == tag)
            .map(|f| f.value.as_str())
            .collect()
    }

    fn encode(&self, out: &mut Vec<u8>) {
        for (i, f) in self.fields.iter().enumerate() {
            if i > 0 {
                out.push(FIELD_CHAR);
            }
            out.extend_from_slice(f.tag.to_string().as_bytes());
            if f.index != 0 {
                out.push(DOT_CHAR);
                out.extend_from_slice(f.index.to_string().as_bytes());
            }
            out.push(EQUAL_CHAR);
            out.extend_from_slice(f.value.as_bytes());
        }
    }
}

/// Decode one field: `<tag>[.<index>]=<value>`.
///
/// Splits on the first `=` (values may contain further `=` bytes),
/// validates UTF-8, and accepts either `tag` or `tag.index` decimal
/// identifiers. Malformed identifiers surface as
/// [`StampError::MissingEquals`].
fn parse_field(bytes: &[u8]) -> Result<StampField, StampError> {
    let eq = bytes
        .iter()
        .position(|&b| b == EQUAL_CHAR)
        .ok_or(StampError::MissingEquals)?;
    let id_part = &bytes[..eq];
    let value = std::str::from_utf8(&bytes[eq + 1..])
        .map_err(|_| StampError::InvalidUtf8)?
        .to_string();
    // Tag may carry an index: "50.1".
    let (tag, index) = match id_part.iter().position(|&b| b == DOT_CHAR) {
        Some(dot) => {
            let tag: i32 = std::str::from_utf8(&id_part[..dot])
                .map_err(|_| StampError::InvalidUtf8)?
                .parse()
                .map_err(|_| StampError::MissingEquals)?;
            let index: i32 = std::str::from_utf8(&id_part[dot + 1..])
                .map_err(|_| StampError::InvalidUtf8)?
                .parse()
                .map_err(|_| StampError::MissingEquals)?;
            (tag, index)
        }
        None => {
            let tag: i32 = std::str::from_utf8(id_part)
                .map_err(|_| StampError::InvalidUtf8)?
                .parse()
                .map_err(|_| StampError::MissingEquals)?;
            (tag, 0)
        }
    };
    Ok(StampField { tag, index, value })
}

/// A complete STAMP message: control header + business + optional trailer.
///
/// Owns every decoded value; there are no lifetimes, so a parsed message
/// can outlive the read buffer and be handed across tasks freely.
/// # Examples
/// ```
/// # use exchange8_stamp::{StampMessage, business_class, tag};
/// let msg = StampMessage::build(
///     business_class::TRADE_CONFIRM,
///     vec![(tag::SYMBOL, "XYZ".to_string())],
/// );
/// let wire = msg.encode();
/// let back = StampMessage::parse(&wire).unwrap();
/// assert_eq!(back, msg); // round trip is lossless
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StampMessage {
    /// Session-level fields (sequence numbers, heartbeat, version).
    pub control_header: StampSection,
    /// Business payload in wire order.
    pub business: StampSection,
    /// Trailing control section; `None` when the frame carries no trailer.
    pub control_trailer: Option<StampSection>,
}

impl StampMessage {
    /// Business-class accessor (`tag 6` in the business section).
    ///
    /// # Examples
    /// ```
    /// # use exchange8_stamp::{StampMessage, business_class};
    /// let msg = StampMessage::build(business_class::ORDER_ACK, vec![]);
    /// assert_eq!(msg.business_class(), Some(business_class::ORDER_ACK));
    /// ```
    pub fn business_class(&self) -> Option<&str> {
        self.business.get(tag::BUSINESS_CLASS)
    }

    /// Parse a full frame.
    ///
    /// The frame layout is `<0x01> header <0x1c> business <0x1d> trailer`.
    /// The control header is mandatory; the business section and trailer
    /// are optional, so header-only frames (heartbeats/session control)
    /// and truncated captures parse without error as long as the leading
    /// byte and the contained fields are well formed.
    ///
    /// # Errors
    ///
    /// - [`StampError::NoControlHeader`] when the buffer is empty or its
    ///   first byte is not `0x01`.
    /// - [`StampError::MissingEquals`] for any field without an `=`
    ///   separator or with a non-numeric tag/index identifier.
    /// - [`StampError::InvalidUtf8`] for non-UTF-8 field content or
    ///   identifiers.
    ///
    /// # Examples
    /// ```
    /// # use exchange8_stamp::{StampMessage, tag, business_class};
    /// let msg = StampMessage::build(
    ///     business_class::ORDER_ACK,
    ///     vec![(tag::SYMBOL, "ABC".to_string())],
    /// );
    /// let parsed = StampMessage::parse(&msg.encode()).unwrap();
    /// assert_eq!(parsed.business.get(tag::SYMBOL), Some("ABC"));
    /// // Malformed (no leading control byte) is a typed error:
    /// assert!(StampMessage::parse(b"55=ABC").is_err());
    /// ```
    pub fn parse(buf: &[u8]) -> Result<Self, StampError> {
        let first = *buf.first().ok_or(StampError::NoControlHeader)?;
        if first != CONTROL_HEADER_CHAR {
            return Err(StampError::NoControlHeader);
        }
        // Find the business separator (optional) and the trailer (optional).
        let business_pos = buf
            .iter()
            .skip(1)
            .position(|&b| b == BUSINESS_CONTENT_CHAR)
            .map(|p| p + 1);
        let trailer_pos = buf
            .iter()
            .skip(1)
            .position(|&b| b == CONTROL_TRAILER_CHAR)
            .map(|p| p + 1);

        let (header_end, business, trailer) = match (business_pos, trailer_pos) {
            (Some(bpos), Some(tpos)) => {
                let bsec_end = tpos.min(buf.len());
                let business = StampSection::parse(&buf[bpos + 1..bsec_end])?;
                let trailer = StampSection::parse(&buf[tpos + 1..])?;
                (bpos, business, Some(trailer))
            }
            (Some(bpos), None) => {
                let business = StampSection::parse(&buf[bpos + 1..])?;
                (bpos, business, None)
            }
            (None, Some(tpos)) => {
                // Header-only message with a trailer.
                let trailer = StampSection::parse(&buf[tpos + 1..])?;
                (tpos, StampSection::default(), Some(trailer))
            }
            (None, None) => (buf.len(), StampSection::default(), None),
        };
        let control_header = StampSection::parse(&buf[1..header_end])?;
        Ok(StampMessage {
            control_header,
            business,
            control_trailer: trailer,
        })
    }

    /// Encode back to wire format.
    ///
    /// Fields are written in stored order; the business delimiter is
    /// emitted only when the business section is non-empty or a trailer
    /// exists, and the trailer delimiter only when a trailer is present.
    ///
    /// # Examples
    /// ```
    /// # use exchange8_stamp::{StampMessage, CONTROL_HEADER_CHAR, BUSINESS_CONTENT_CHAR};
    /// let msg = StampMessage::build("010", vec![]);
    /// let wire = msg.encode();
    /// assert_eq!(wire[0], CONTROL_HEADER_CHAR);
    /// assert!(wire.contains(&BUSINESS_CONTENT_CHAR));
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(CONTROL_HEADER_CHAR);
        self.control_header.encode(&mut out);
        if !self.business.fields.is_empty() || self.control_trailer.is_some() {
            out.push(BUSINESS_CONTENT_CHAR);
            self.business.encode(&mut out);
        }
        if let Some(trailer) = &self.control_trailer {
            out.push(CONTROL_TRAILER_CHAR);
            trailer.encode(&mut out);
        }
        out
    }

    /// Build a message with a business class and unindexed fields.
    ///
    /// The class discriminator is written as the first business field
    /// (`tag 6`), followed by the supplied pairs in order, all with
    /// index 0. The built message has an empty control header and no
    /// trailer; add session fields by mutating [`StampMessage`]'s sections
    /// as needed.
    ///
    /// # Examples
    /// ```
    /// # use exchange8_stamp::{StampMessage, tag, business_class};
    /// let msg = StampMessage::build(
    ///     business_class::ORDER_REQUEST,
    ///     vec![(tag::SYMBOL, "ABC".to_string()), (tag::VOLUME, "100".to_string())],
    /// );
    /// assert_eq!(msg.business_class(), Some("001"));
    /// assert_eq!(msg.business.get(tag::VOLUME), Some("100"));
    /// ```
    pub fn build(business_class: &str, fields: Vec<(i32, String)>) -> Self {
        let mut business = StampSection::default();
        business.fields.push(StampField {
            tag: tag::BUSINESS_CLASS,
            index: 0,
            value: business_class.to_string(),
        });
        for (t, v) in fields {
            business.fields.push(StampField {
                tag: t,
                index: 0,
                value: v,
            });
        }
        StampMessage {
            control_header: StampSection::default(),
            business,
            control_trailer: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg_with_header(business_fields: &[(i32, &str)]) -> Vec<u8> {
        let mut m = StampMessage::build("010", vec![]);
        for (t, v) in business_fields {
            m.business.fields.push(StampField {
                tag: *t,
                index: 0,
                value: v.to_string(),
            });
        }
        // Control header with sequence  version (framing).
        m.control_header.fields = vec![
            StampField {
                tag: tag::SEQUENCE_NUMBER,
                index: 0,
                value: "42".into(),
            },
            StampField {
                tag: tag::VERSION_NUMBER,
                index: 0,
                value: STAMP_VERSION_NUMBER.into(),
            },
        ];
        m.control_trailer = Some(StampSection::default());
        m.encode()
    }

    #[test]
    fn control_chars_match_spec() {
        assert_eq!(CONTROL_HEADER_CHAR, 0x01);
        assert_eq!(BUSINESS_CONTENT_CHAR, 0x1c);
        assert_eq!(CONTROL_TRAILER_CHAR, 0x1d);
        assert_eq!(FIELD_CHAR, 0x1e);
    }

    #[test]
    fn round_trip_full_message() {
        let wire = msg_with_header(&[
            (tag::SYMBOL, "ABC"),
            (tag::ORDER_NUMBER, "1001"),
            (tag::PRICE, "10.05"),
            (tag::VOLUME, "100"),
        ]);
        let parsed = StampMessage::parse(&wire).unwrap();
        assert_eq!(parsed.business_class(), Some("010"));
        assert_eq!(parsed.business.get(tag::SYMBOL), Some("ABC"));
        assert_eq!(parsed.business.get(tag::ORDER_NUMBER), Some("1001"));
        assert_eq!(parsed.business.get(tag::PRICE), Some("10.05"));
        assert_eq!(parsed.business.get(tag::VOLUME), Some("100"));
        assert_eq!(parsed.control_header.get(tag::SEQUENCE_NUMBER), Some("42"));
        assert_eq!(
            parsed.control_header.get(tag::VERSION_NUMBER),
            Some(STAMP_VERSION_NUMBER)
        );
        // Trailer present (empty section).
        assert!(parsed.control_trailer.is_some());
        // Re-encode → identical bytes.
        assert_eq!(parsed.encode(), wire);
    }

    #[test]
    fn section_field_separator_is_rs() {
        let wire = msg_with_header(&[(tag::SYMBOL, "X"), (tag::VOLUME, "1")]);
        // Fields joined by 0x1e between values.
        let sep_pos = wire
            .windows(2)
            .position(|w| w == [FIELD_CHAR, b'6'])
            .unwrap();
        assert_eq!(wire[sep_pos], FIELD_CHAR);
    }

    #[test]
    fn indexed_fields_preserved() {
        // InsertStringField(tag, index): "50.1=a" and "50.2=b".
        let mut m = StampMessage::build("008", vec![]);
        m.business.fields.push(StampField {
            tag: tag::SEQUENCE_NUMBER,
            index: 1,
            value: "a".into(),
        });
        m.business.fields.push(StampField {
            tag: tag::SEQUENCE_NUMBER,
            index: 2,
            value: "b".into(),
        });
        let wire = m.encode();
        let parsed = StampMessage::parse(&wire).unwrap();
        assert_eq!(
            parsed.business.get_indexed(tag::SEQUENCE_NUMBER, 1),
            Some("a")
        );
        assert_eq!(
            parsed.business.get_indexed(tag::SEQUENCE_NUMBER, 2),
            Some("b")
        );
        // get_all returns in stored order.
        assert_eq!(
            parsed.business.get_all(tag::SEQUENCE_NUMBER),
            vec!["a", "b"]
        );
        assert_eq!(parsed.encode(), wire);
    }

    #[test]
    fn header_only_message() {
        let mut m = StampMessage::default();
        m.control_header.fields.push(StampField {
            tag: tag::HEARTBEAT_PERIOD,
            index: 0,
            value: "30".into(),
        });
        let wire = m.encode();
        let parsed = StampMessage::parse(&wire).unwrap();
        assert_eq!(parsed.control_header.get(tag::HEARTBEAT_PERIOD), Some("30"));
        assert!(parsed.business.fields.is_empty());
        assert!(parsed.control_trailer.is_none());
    }

    #[test]
    fn missing_control_header_rejected() {
        assert_eq!(
            StampMessage::parse(b"50=1"),
            Err(StampError::NoControlHeader)
        );
        assert_eq!(StampMessage::parse(b""), Err(StampError::NoControlHeader));
    }

    #[test]
    fn field_without_equals_rejected() {
        // Header "50 1" - no '=' → throws StampException.
        let wire: Vec<u8> = vec![CONTROL_HEADER_CHAR, b'5', b'0', FIELD_CHAR, b'1'];
        assert_eq!(StampMessage::parse(&wire), Err(StampError::MissingEquals));
    }

    #[test]
    fn empty_business_section_ok() {
        // Header, business delimiter, trailer - all empty sections.
        let wire: Vec<u8> = vec![
            CONTROL_HEADER_CHAR,
            BUSINESS_CONTENT_CHAR,
            CONTROL_TRAILER_CHAR,
        ];
        let parsed = StampMessage::parse(&wire).unwrap();
        assert!(parsed.control_header.fields.is_empty());
        assert!(parsed.business.fields.is_empty());
        assert!(parsed.control_trailer.is_some());
    }

    #[test]
    fn business_and_trailer_offsets() {
        // Verify sections split at the right delimiters: a value containing
        // the trailer char inside the BUSINESS section must not truncate it…
        // (memchr scans sequentially; business first, then trailer).
        let mut m = StampMessage::build("008", vec![]);
        m.business.fields.push(StampField {
            tag: tag::SYMBOL,
            index: 0,
            value: "ABC".into(),
        });
        m.control_header.fields.push(StampField {
            tag: tag::SEQUENCE_NUMBER,
            index: 0,
            value: "7".into(),
        });
        m.control_trailer = Some(StampSection::default());
        let wire = m.encode();
        // Layout: 0x01 | "50=7" | 0x1c | "6=008 55=ABC" | 0x1d
        assert_eq!(wire[0], CONTROL_HEADER_CHAR);
        let bpos = wire
            .iter()
            .position(|&b| b == BUSINESS_CONTENT_CHAR)
            .unwrap();
        let tpos = wire
            .iter()
            .position(|&b| b == CONTROL_TRAILER_CHAR)
            .unwrap();
        assert!(bpos < tpos);
        // Round-trip preserves everything.
        let parsed = StampMessage::parse(&wire).unwrap();
        assert_eq!(parsed.control_header.get(tag::SEQUENCE_NUMBER), Some("7"));
        assert_eq!(parsed.business.get(tag::SYMBOL), Some("ABC"));
    }

    #[test]
    fn trade_confirm_message_shape() {
        // A realistic venue trade-confirm (business class 008).
        let m = StampMessage::build(
            business_class::TRADE_CONFIRM,
            vec![
                (tag::SYMBOL, "XYZ".into()),
                (tag::CASH_TRADE_ID, "9001".into()),
                (tag::LAST_FILL_PRICE, "10.25".into()),
                (tag::FILL_VOLUME, "500".into()),
                (tag::BUY_BROKER_NUMBER, "101".into()),
                (tag::SELL_BROKER_NUMBER, "202".into()),
            ],
        );
        let wire = m.encode();
        let parsed = StampMessage::parse(&wire).unwrap();
        assert_eq!(parsed.business_class(), Some("008"));
        assert_eq!(parsed.business.get(tag::CASH_TRADE_ID), Some("9001"));
        assert_eq!(parsed.business.get(tag::LAST_FILL_PRICE), Some("10.25"));
        assert_eq!(parsed.business.get(tag::FILL_VOLUME), Some("500"));
    }

    #[test]
    fn order_request_message_shape() {
        let m = StampMessage::build(
            business_class::ORDER_REQUEST,
            vec![
                (tag::SYMBOL, "ABC".into()),
                (tag::ORDER_SIDE, "1".into()),
                (tag::VOLUME, "300".into()),
                (tag::ORDER_TYPE, "2".into()),
                (tag::ORDER_NUMBER, "77".into()),
                (tag::BROKER_NUMBER, "42".into()),
            ],
        );
        let parsed = StampMessage::parse(&m.encode()).unwrap();
        assert_eq!(parsed.business_class(), Some("001"));
        assert_eq!(parsed.business.get(tag::ORDER_SIDE), Some("1"));
        assert_eq!(parsed.business.get(tag::BROKER_NUMBER), Some("42"));
    }

    #[test]
    fn spec_tag_values() {
        // Pinned tag numbers.
        assert_eq!(tag::SYMBOL, 55);
        assert_eq!(tag::PRICE, 41);
        assert_eq!(tag::VOLUME, 64);
        assert_eq!(tag::SEQUENCE_NUMBER, 50);
        assert_eq!(tag::VERSION_NUMBER, 65);
        assert_eq!(tag::CASH_TRADE_ID, 86);
        assert_eq!(tag::BUY_BROKER_NUMBER, 83);
        assert_eq!(tag::SELL_BROKER_NUMBER, 84);
    }
}
