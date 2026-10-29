// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 minimal FIX 4.2 tagvalue codec - the wire-format data plane of
//! the FIX order-entry path.
//!
//! The crate is a pure, synchronous library: it represents a FIX message as
//! an ordered [`FixMessage`] field list, writes complete frames
//! (`8=…9=…35=……10=nnn|`) with [`FixMessage::encode`], and reads byte
//! streams back into validated messages with [`FixMessage::parse`]. It
//! speaks the exact platform framing dialect: `BodyLength` covers the body
//! **plus** the `10=` trailer, and the CheckSum is the modulo-256 byte sum
//! of everything before `10=`.
//!
//! It owns no transport, no session state, and no persistence: the FIX
//! session state machine (logon/gap/resend/logout) lives in Exchange8:Gateway
//! (`services/gateway/src/session.rs`), which consumes this crate through
//! the workspace facade (`exchange8::fix`).
//!
//! # Quick-start
//!
//! ```
//! use exchange8_fix::{FixMessage, msg_type, tag};
//!
//! let mut order = FixMessage::typed(msg_type::NEW_ORDER);
//! order
//!     .set(tag::CL_ORD_ID, "O-1")
//!     .set(tag::SYMBOL, "ACME")
//!     .set(tag::SIDE, "1")
//!     .set(tag::ORDER_QTY, 100)
//!     .set(tag::ORD_TYPE, "2")
//!     .set(tag::PRICE, "10.50");
//! let frame = order.encode();
//! let parsed = FixMessage::parse(&frame).unwrap().unwrap();
//! assert_eq!(parsed.msg_type(), Some(msg_type::NEW_ORDER));
//! assert_eq!(parsed.get(tag::PRICE), Some("10.50"));
//! ```
//!
//! # Framing math (the platform wire spec)
//!
//! - encode: `BodyLength = body.len() + 7` (the trailer `10=xxx` + SOH is
//!   7 bytes); the `9=` field is inserted right after `8=`.
//! - parse: `computed = frame.len() - body_start` - BodyLength INCLUDES the
//!   `10=` field; checksum = sum(frame[..ck_start]) % 256 (everything
//!   before `10=`).
//!
//! # Reference wire anchors
//!
//! - `design/behavior-catalog/wire-formats.json` (framing ground truth).
//! - The reference platform's `Library/Fix/FixConst.h` (`FIX_42_BEG_STR`,
//!   tag vocabulary) and `FixMsgParser.cpp` (parser semantics).
//!
//! Design: `design/sentin8-exchange8-gateway-technical-design.md` §4.2,
//! and `crates/fix/DESIGN.md` for the detailed wire-level design.

use thiserror::Error;

/// FIX field separator (start-of-heading) - the sole field delimiter on the
/// wire; every `tag=value` pair on the wire ends with this byte.
pub const SOH: u8 = 0x01;

/// BeginString (8) value written on every frame this codec encodes.
///
/// The `FIX_42_BEG_STR` constant of the reference platform (`FixConst.h:22`).
pub const FIX_42_BEGIN_STRING: &str = "FIX.4.2";

/// Default HeartBtInt (108) in seconds applied when a logon omits tag 108.
pub const DEFT_HEART_BT_INT: u32 = 30;

/// Lower clamp for the negotiated HeartBtInt (108): requests below 10 s are
/// raised to this value by the session shell.
pub const MIN_HEART_BT_INT: u32 = 10;

/// Upper clamp for the negotiated HeartBtInt (108): requests above 120 s are
/// reduced to this value by the session shell.
pub const MAX_HEART_BT_INT: u32 = 120;

/// Parse/encode failure taxonomy for [`FixMessage::parse`].
///
/// Every variant is a hard structural reject: a frame failing any check
/// yields an `Err` and nothing is delivered to the caller, so framing
/// defects (corrupted length, corrupted checksum) can never be decoded into
/// a half-valid [`FixMessage`]. Truncated-but-recognizable frames are not
/// errors at all - [`FixMessage::parse`] reports them as `Ok(None)` so the
/// caller keeps buffering.
///
/// # Examples
/// ```
/// use exchange8_fix::{FixError, FixMessage};
///
/// assert!(matches!(
///     FixMessage::parse(b"not-a-fix-frame"),
///     Err(FixError::TooShort)
/// ));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FixError {
    /// Frame is shorter than the minimal FIX 4.2 shape (16 bytes) and does
    /// not even start with `8=`, so it cannot be a truncated FIX frame.
    #[error("frame too short")]
    TooShort,
    /// The first field is not `8=FIX.4.2` - BeginString missing or wrong
    /// protocol version.
    #[error("missing BeginString")]
    NoBeginString,
    /// A field before the CheckSum is malformed: no `=`, a non-numeric tag,
    /// or the second field is not `9=` (BodyLength). The payload carries the
    /// field index (0-based) where parsing failed.
    #[error("bad field at index {0}")]
    BadField(usize),
    /// Declared BodyLength (tag 9) does not equal the computed length.
    #[error("BodyLength mismatch: declared {declared}, computed {computed}")]
    BodyLength { declared: u32, computed: u32 },
    /// Declared CheckSum (tag 10) does not equal the modulo-256 byte sum.
    #[error("checksum mismatch: declared {declared}, computed {computed}")]
    Checksum { declared: u8, computed: u8 },
}

/// MsgType (35) values spoken by Exchange8:Gateway (the platform wire spec).
///
/// Session-administration messages (`0`-`5`, `A`) follow FIX 4.2 session
/// semantics; application messages cover order entry (`D`/`F`/`G`/`H`),
/// execution lifecycle (`8`/`9`/`j`), and the FX RFQ quote family
/// (`R`/`s`/`aj`/`AJ` - `design/sentin8-exchange8-fx-technical-design.md`
/// §3.6).
///
/// # Examples
/// ```
/// use exchange8_fix::{FixMessage, msg_type};
///
/// let heart = FixMessage::typed(msg_type::HEARTBEAT);
/// assert_eq!(heart.msg_type(), Some(msg_type::HEARTBEAT));
/// ```
pub mod msg_type {
    /// Heartbeat (0) - session keepalive probe.
    pub const HEARTBEAT: &str = "0";
    /// TestRequest (1) - forces a Heartbeat reply echoing TestReqID (112).
    pub const TEST_REQUEST: &str = "1";
    /// ResendRequest (2) - retransmit a sequence range (BeginSeqNo/EndSeqNo).
    pub const RESEND_REQUEST: &str = "2";
    /// Reject (3) - session-level reject (RefSeqNum 45, RefTagID 371,
    /// SessionRejectReason 373).
    pub const REJECT: &str = "3";
    /// SequenceReset (4) - sequence reset / GapFill (GapFillFlag 123,
    /// NewSeqNo 36).
    pub const SEQUENCE_RESET: &str = "4";
    /// Logout (5) - session teardown notice.
    pub const LOGOUT: &str = "5";
    /// Logon (A) - session establish (HeartBtInt 108 negotiation,
    /// ResetSeqNumFlag 141).
    pub const LOGON: &str = "A";
    /// NewOrderSingle (D) - order entry.
    pub const NEW_ORDER: &str = "D";
    /// OrderCancelRequest (F).
    pub const CANCEL_REQUEST: &str = "F";
    /// OrderCancelReplaceRequest (G).
    pub const REPLACE_REQUEST: &str = "G";
    /// OrderStatusRequest (H).
    pub const ORDER_STATUS_REQUEST: &str = "H";
    /// ExecutionReport (8) - order lifecycle report (ExecType 150,
    /// OrdStatus 39).
    pub const EXECUTION_REPORT: &str = "8";
    /// OrderCancelReject (9).
    pub const CANCEL_REJECT: &str = "9";
    /// BusinessMessageReject (j).
    pub const BUSINESS_REJECT: &str = "j";
    // -- FIX 4.4 FX quote flow (design §3.6) ----------------------
    /// QuoteRequest (R) - client → venue RFQ.
    pub const QUOTE_REQUEST: &str = "R";
    /// Quote (s) - venue → client tradeable price.
    pub const QUOTE: &str = "s";
    /// QuoteResponse (aj) - client accepts / counters a quote.
    pub const QUOTE_RESPONSE: &str = "aj";
    /// QuoteStatusReport (AJ) - venue → client terminal state.
    pub const QUOTE_STATUS_REPORT: &str = "AJ";
}

/// Standard FIX 4.2 tag constants used by the codec and Exchange8:Gateway
/// (the platform wire spec, `FixTags` of the reference platform), plus the
/// venue-dialect custom tags (6749-9730) and the FIX 4.4 FX quote family
/// (§3.6). Values are FIX tag numbers ([`i32`]); fields are addressed by
/// these constants with [`FixMessage::set`]/[`FixMessage::get`].
///
/// # Examples
/// ```
/// use exchange8_fix::{FixMessage, tag};
///
/// let mut m = FixMessage::new();
/// m.set(tag::SYMBOL, "ACME");
/// assert_eq!(m.get(tag::SYMBOL), Some("ACME"));
/// assert_eq!(tag::MSG_TYPE, 35);
/// ```
pub mod tag {
    /// Account (1) - submitting account.
    pub const ACCOUNT: i32 = 1;
    /// AvgPx (6) - average execution price on the order.
    pub const AVG_PX: i32 = 6;
    /// BeginString (8) - FIX protocol version, always first on the frame.
    pub const BEGIN_STRING: i32 = 8;
    /// BodyLength (9) - second field; covers the body plus the CheckSum
    /// trailer on this codec's frames.
    pub const BODY_LENGTH: i32 = 9;
    /// CheckSum (10) - trailer; modulo-256 byte sum, always last on the frame.
    pub const CHECKSUM: i32 = 10;
    /// ClOrdID (11) - client order id.
    pub const CL_ORD_ID: i32 = 11;
    /// CumQty (14) - total executed on the order.
    pub const CUM_QTY: i32 = 14;
    /// ExecID (17) - execution id (the trade id on fills).
    pub const EXEC_ID: i32 = 17;
    /// ExecInst (18) - execution instruction flags (peg variants, dark,
    /// post-only, all-or-none, bypass).
    pub const EXEC_INST: i32 = 18;
    /// ExecTransType (20) - 0=new, 1=cancel (bust), 2=correct (amend).
    pub const EXEC_TRANS_TYPE: i32 = 20;
    /// HandlInst (21) - order handling instruction.
    pub const HANDL_INST: i32 = 21;
    /// IDSource (22) - security identifier class (symbol/ISIN/SEDOL).
    pub const ID_SOURCE: i32 = 22;
    /// SecurityID (48) - alternate security identifier.
    pub const SECURITY_ID: i32 = 48;
    /// SenderCompID (49) - the sending party's CompID.
    pub const SENDER_COMP_ID: i32 = 49;
    /// SenderSubID (50) - sub-identifier (desk/login) of the sender.
    pub const SENDER_SUB_ID: i32 = 50;
    /// SendingTime (52) - UTCTimestamp of transmission.
    pub const SENDING_TIME: i32 = 52;
    /// Side (54) - 1=buy, 2=sell (base quantity).
    pub const SIDE: i32 = 54;
    /// Symbol (55) - instrument symbol (currency pair for FX).
    pub const SYMBOL: i32 = 55;
    /// TargetCompID (56) - the receiving party's CompID.
    pub const TARGET_COMP_ID: i32 = 56;
    /// Text (58) - free-form human context (reject reason, tenor carrier).
    pub const TEXT: i32 = 58;
    /// TimeInForce (59) - 0=day, 1=GTC, 2=at-open, 3=IOC, 4=FOK, 6=crossing,
    /// 9=GTD (the platform's order-shape vocabulary).
    pub const TIME_IN_FORCE: i32 = 59;
    /// TransactTime (60) - business event time.
    pub const TRANSACT_TIME: i32 = 60;
    /// ExecBroker (76) - executing broker CompID.
    pub const EXEC_BROKER: i32 = 76;
    /// OrderID (37) - venue-assigned order id.
    pub const ORDER_ID: i32 = 37;
    /// OrderQty (38) - ordered quantity (QuoteRequest leg quantity for 35=R).
    pub const ORDER_QTY: i32 = 38;
    /// OrdStatus (39) - 0=new, 1=partial, 2=filled, 4=canceled, 5=replaced,
    /// 8=rejected.
    pub const ORD_STATUS: i32 = 39;
    /// OrdType (40) - 2=limit, P=pegged (the platform's order types).
    pub const ORD_TYPE: i32 = 40;
    /// OrigClOrdID (41) - prior client order id on cancel/replace.
    pub const ORIG_CL_ORD_ID: i32 = 41;
    /// PossDupFlag (43) - `Y` on re-delivered session traffic.
    pub const POSS_DUP: i32 = 43;
    /// Price (44) - limit price (decimal string on the wire).
    pub const PRICE: i32 = 44;
    /// BeginSeqNo (7) - first sequence of a ResendRequest range.
    pub const BEGIN_SEQ_NO: i32 = 7;
    /// EndSeqNo (16) - last sequence of a ResendRequest range (0 = infinity).
    pub const END_SEQ_NO: i32 = 16;
    /// NewSeqNo (36) - new sequence number on SequenceReset/GapFill.
    pub const NEW_SEQ_NO: i32 = 36;
    /// LastPx (31) - price of the last fill.
    pub const LAST_PX: i32 = 31;
    /// LastQty (32) - quantity of the last fill (reused as deal size on
    /// tradeable quotes, `LAST_QTY_QUOTE`).
    pub const LAST_QTY: i32 = 32;
    /// LastMkt (30) - market of the last fill (4-byte book code).
    pub const LAST_MKT: i32 = 30;
    /// MsgSeqNum (34) - session sequence number.
    pub const MSG_SEQ_NUM: i32 = 34;
    /// MsgType (35) - third header field; selects the message body schema.
    pub const MSG_TYPE: i32 = 35;
    /// Cancel-on-disconnect dialect slot (tag 58, colliding with Text);
    /// cancel-on-disconnect is applied per connection by the session shell,
    /// never carried per-message on this codec's path.
    pub const CANCEL_ON_DISCONNECT: i32 = 58;
    /// PossResend (97) - possible duplicate resend marker.
    pub const POSS_RESEND: i32 = 97;
    /// EncryptedMethod (98) - logon schema marker; always 0 - the transport
    /// provides encryption outside the message body.
    pub const ENCRYPT_METHOD: i32 = 98;
    /// ExDestination (100) - routing destination (SelectStrategy marker).
    pub const EX_DESTINATION: i32 = 100;
    /// CxlRejReason (102) - cancel-reject reason code.
    pub const CXL_REJ_REASON: i32 = 102;
    /// OrdRejReason (103) - order reject reason code.
    pub const ORD_REJ_REASON: i32 = 103;
    /// HeartBtInt (108) - heartbeat interval in seconds (logon negotiation).
    pub const HEART_BT_INT: i32 = 108;
    /// ClientID (109) - client identifier.
    pub const CLIENT_ID: i32 = 109;
    /// MinQty (110) - minimum execution quantity.
    pub const MIN_QTY: i32 = 110;
    /// MaxFloor (111) - iceberg display quantity.
    pub const MAX_FLOOR: i32 = 111;
    /// TestReqID (112) - echoed TestRequest identifier.
    pub const TESTREQ_ID: i32 = 112;
    /// OnBehalfOfCompID (115) - vendor submitting for a beneficiary firm.
    pub const ON_BEHALF_OF_COMP_ID: i32 = 115;
    /// OrigSendingTime (122) - original send time on retransmissions.
    pub const ORIG_SENDING_TIME: i32 = 122;
    /// GapFillFlag (123) - `Y` marks a SequenceReset as gap-fill.
    pub const GAP_FILL_FLAG: i32 = 123;
    /// ResetSeqNumFlag (141) - `Y` restarts both counters at 1 on logon.
    pub const RESET_SEQ_NUM_FLAG: i32 = 141;
    /// ExecType (150) - execution-report kind (0=new, 4=canceled, 5=replace,
    /// 8=rejected, F=trade).
    pub const EXEC_TYPE: i32 = 150;
    /// LeavesQty (151) - open quantity remaining.
    pub const LEAVES_QTY: i32 = 151;
    /// SecurityExchange (207) - exchange/market identifier.
    pub const SECURITY_EXCHANGE: i32 = 207;
    /// PegDifference (211) - offset from the peg reference price.
    pub const PEG_DIFFERENCE: i32 = 211;
    /// RefSeqNum (45) - sequence the session Reject refers to.
    pub const REF_SEQ_NUM: i32 = 45;
    /// RefTagID (371) - offending tag referenced by a session Reject.
    pub const REF_TAG_ID: i32 = 371;
    /// RefMsgType (372) - message type referenced by a session Reject.
    pub const REF_MSG_TYPE: i32 = 372;
    /// SessionRejectReason (373) - session-reject reason code.
    pub const SESSION_REJECT_REASON: i32 = 373;
    // -- Venue-dialect custom tags (`FixConst.h:137-171`) ----------
    /// STP no-match id (6749) - self-cross prevention token.
    pub const UMIR_NO_MATCH_ID: i32 = 6749;
    /// UMIR account type (6750).
    pub const UMIR_ACCOUNT_TYPE_ID: i32 = 6750;
    /// UMIR user id (6751).
    pub const UMIR_USER_ID: i32 = 6751;
    /// Program-trade marker (6755).
    pub const PROGRAM_TRADE: i32 = 6755;
    /// UMIR jitney flag (6757).
    pub const UMIR_JITNEY: i32 = 6757;
    /// Anonymous-order marker (6761).
    pub const ANONYMOUS: i32 = 6761;
    /// UMIR regulation id (6763).
    pub const UMIR_REGULATION_ID: i32 = 6763;
    /// Bypass marker (6791) - skip price-protection on this order.
    pub const BYPASS_MARKER: i32 = 6791;
    /// Price-protection marker (6820) - `K` = cancel-on-protect-trigger.
    pub const PROTECTION: i32 = 6820;
    /// Protection price-improvement option (6821).
    pub const PROTECTION_PRICE_IMPROVEMENT: i32 = 6821;
    /// Book code (6888) - board/odd book selector (4-char code, e.g. XL8M).
    pub const BOOK_CODE: i32 = 6888;
    /// Match strategy (6889) - book matching algorithm selector.
    pub const MATCH_STRATEGY: i32 = 6889;
    /// Post-only override (9303).
    pub const ORDER_POST_ONLY: i32 = 9303;
    /// Visible/hidden display override (9479).
    pub const ORDER_VISIBLE: i32 = 9479;
    /// EBBO improvement flag (9500).
    pub const EBBO_FLAG: i32 = 9500;
    /// Routing strategy token (9501).
    pub const ROUTING_STRATEGY: i32 = 9501;
    /// Liquidity flag (9730) - added/removed on ExecutionReport fills.
    pub const LIQUIDITY_FLAG: i32 = 9730;
    // -- FIX 4.4 FX quote / RFQ tags (fx design §3.6) ---------------
    /// QuoteReqID (131) - correlates the QuoteRequest/Quote/QuoteResponse/
    /// QuoteStatusReport chain.
    pub const QUOTE_REQ_ID: i32 = 131;
    /// QuoteID (299) - the venue's per-quote id (35=s, 35=AJ).
    pub const QUOTE_ID: i32 = 299;
    /// QuoteRespID (693) - the LP's bid/accept id (35=aj QuoteResponse).
    pub const QUOTE_RESP_ID: i32 = 693;
    /// QuoteRespType (694) - 0=accept/hit, 1=counter.
    pub const QUOTE_RESP_TYPE: i32 = 694;
    /// QuoteType (537) - 1=tradeable, 2=indicative.
    pub const QUOTE_TYPE: i32 = 537;
    /// QuoteStatus (297) - 0=accepted, 4=expired, 7=rejected, 9=cancelled.
    pub const QUOTE_STATUS: i32 = 297;
    /// BidPx (132) - the LP's bid on a tradeable two-way quote.
    pub const BID_PX: i32 = 132;
    /// OfferPx (133) - the LP's ask on a tradeable two-way quote.
    pub const OFFER_PX: i32 = 133;
    /// ValidUntilTime (62) - quote expiry (bounded by the RFQ engine's
    /// `MAX_VALID_WINDOW_US`).
    pub const VALID_UNTIL_TIME: i32 = 62;
    /// SettlDate (64) - value date (T+1/T+2 spot, forward dates).
    pub const SETTL_DATE: i32 = 64;
    /// Currency (15) - quote currency (NDF netting currency).
    pub const CURRENCY: i32 = 15;
    /// OrderQty (38) reused as QuoteRequest leg quantity.
    pub const QUOTE_REQ_QTY: i32 = 38;
    /// FutSettDate2 (193) - FX swap far-leg value date.
    pub const FUT_SETT_DATE_2: i32 = 193;
    /// LastQty (32) reused as the deal size on tradeable quotes.
    pub const LAST_QTY_QUOTE: i32 = 32;
}

/// A parsed FIX message: an **ordered** list of `(tag, value)` fields.
///
/// The list is the source of truth - the codec is deliberately
/// schema-less. `encode()` derives the framing (BeginString, BodyLength,
/// MsgType-first ordering, CheckSum) from the field list; `parse()` fills
/// the list in wire order. Tag ordering is preserved on round-trip apart
/// from MsgType (35), which `encode` hoists to the front of the body per
/// the FIX header rule. Repeating tags (e.g. repeating groups) are stored
/// as separate list entries; `get`/`set` operate on the **first**
/// occurrence.
///
/// # Examples
/// ```
/// use exchange8_fix::{FixMessage, tag};
///
/// let mut m = FixMessage::new();
/// m.set(tag::CL_ORD_ID, "O-42");
/// assert_eq!(m.get(tag::CL_ORD_ID), Some("O-42"));
/// assert_eq!(m.fields.len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FixMessage {
    /// Ordered `(tag, value)` field list in wire order (values are lossy
    /// UTF-8 of the raw bytes between `=` and SOH).
    pub fields: Vec<(i32, String)>,
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().map(|b| *b as u32).sum::<u32>() as u8
}

impl FixMessage {
    /// Empty message (`Default` alias).
    ///
    /// Add fields with [`FixMessage::set`]; use [`FixMessage::typed`] to
    /// start with MsgType (35) preset.
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::FixMessage;
    ///
    /// assert!(FixMessage::new().fields.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience constructor: empty message with MsgType (35) preset.
    ///
    /// `typed(msg_type)` places tag 35 first in the field list, which
    /// [`FixMessage::encode`] then writes as the first body field per the
    /// FIX header rule.
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::{FixMessage, msg_type};
    ///
    /// let logon = FixMessage::typed(msg_type::LOGON);
    /// assert_eq!(logon.msg_type(), Some(msg_type::LOGON));
    /// ```
    pub fn typed(msg_type: &str) -> Self {
        let mut m = Self::default();
        m.set(tag::MSG_TYPE, msg_type);
        m
    }

    /// FIX 4.4 QuoteRequest (35=R): a client-side RFQ for a currency pair.
    ///
    /// `qty` is in base-currency units; `tenor` uses the FX design's
    /// `fx_tenor` codes (SP, SN, 1W, 1M, ...); `side` is 1=buy base, 2=sell
    /// base (FIX Side values). The tenor rides tag 58 (Text) per the FX
    /// design's tag-reuse convention.
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::FixMessage;
    ///
    /// let m = FixMessage::quote_request("QR-17", "EUR/USD", "1", 1_000_000, "SP");
    /// assert_eq!(m.msg_type(), Some("R"));
    /// assert_eq!(m.get(131), Some("QR-17")); // QuoteReqID
    /// ```
    pub fn quote_request(
        quote_req_id: &str,
        symbol: &str,
        side: &str,
        qty: u64,
        tenor: &str,
    ) -> Self {
        let mut m = Self::typed(msg_type::QUOTE_REQUEST);
        m.set(tag::QUOTE_REQ_ID, quote_req_id);
        m.set(tag::SYMBOL, symbol);
        m.set(tag::SIDE, side);
        m.set(tag::ORDER_QTY, qty);
        m.set(tag::TEXT, tenor);
        m
    }

    /// FIX 4.4 Quote (35=s): the venue's tradeable two-way price for a
    /// QuoteReqID.
    ///
    /// Prices are decimal strings; `valid_until` is a FIX UTCTimestamp
    /// string (e.g. `20250115-14:30:00.123`); `settl_date` carries the value
    /// date when the pair's tenor requires one. Bid/ask are the LP-side best
    /// quote after the RFQ engine's last-look binding; the quote is stamped
    /// QuoteType (537) = 1 (tradeable).
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::FixMessage;
    ///
    /// let m = FixMessage::quote("QR-17", "Q-88", "EUR/USD", "1.08450", "1.08456", 1_000_000, "20250115-14:30:00.123", None);
    /// assert_eq!(m.msg_type(), Some("s"));
    /// assert_eq!(m.get(537), Some("1")); // tradeable
    /// ```
    pub fn quote(
        quote_req_id: &str,
        quote_id: &str,
        symbol: &str,
        bid_px: &str,
        offer_px: &str,
        qty: u64,
        valid_until: &str,
        settl_date: Option<&str>,
    ) -> Self {
        let mut m = Self::typed(msg_type::QUOTE);
        m.set(tag::QUOTE_REQ_ID, quote_req_id);
        m.set(tag::QUOTE_ID, quote_id);
        m.set(tag::SYMBOL, symbol);
        m.set(tag::BID_PX, bid_px);
        m.set(tag::OFFER_PX, offer_px);
        m.set(tag::QUOTE_TYPE, 1); // tradeable
        m.set(tag::ORDER_QTY, qty);
        m.set(tag::VALID_UNTIL_TIME, valid_until);
        if let Some(sd) = settl_date {
            m.set(tag::SETTL_DATE, sd);
        }
        m
    }

    /// FIX 4.4 QuoteResponse (35=aj): the client hits (QuoteRespType 694 =
    /// 0) the tradeable quote as-is.
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::FixMessage;
    ///
    /// let m = FixMessage::quote_accept("QR-17", "Q-88");
    /// assert_eq!(m.msg_type(), Some("aj"));
    /// assert_eq!(m.get(694), Some("0"));
    /// ```
    pub fn quote_accept(quote_req_id: &str, quote_id: &str) -> Self {
        let mut m = Self::typed(msg_type::QUOTE_RESPONSE);
        m.set(tag::QUOTE_REQ_ID, quote_req_id);
        m.set(tag::QUOTE_ID, quote_id);
        m.set(tag::QUOTE_RESP_TYPE, 0); // accept
        m
    }

    /// FIX 4.4 QuoteStatusReport (35=AJ): terminal RFQ state - QuoteStatus
    /// (297) `0`=accepted (traded), `7`=rejected (last-look miss or window
    /// expiry), `4`=expired. The optional `reason` rides tag 58 (Text).
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::FixMessage;
    ///
    /// let m = FixMessage::quote_status("QR-17", "Q-88", "7", Some("last-look"));
    /// assert_eq!(m.msg_type(), Some("AJ"));
    /// assert_eq!(m.get(297), Some("7"));
    /// ```
    pub fn quote_status(
        quote_req_id: &str,
        quote_id: &str,
        status: &str,
        reason: Option<&str>,
    ) -> Self {
        let mut m = Self::typed(msg_type::QUOTE_STATUS_REPORT);
        m.set(tag::QUOTE_REQ_ID, quote_req_id);
        m.set(tag::QUOTE_ID, quote_id);
        m.set(tag::QUOTE_STATUS, status);
        if let Some(r) = reason {
            m.set(tag::TEXT, r);
        }
        m
    }

    /// Set (replace on first occurrence) a field.
    ///
    /// If the tag already exists, its **first** occurrence's value is
    /// replaced in place (position preserved); otherwise the field is
    /// appended. Returns `&mut Self` so calls chain. Values convert via
    /// [`ToString`], so integers and prices can be passed directly - the
    /// wire representation stays a string either way.
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::{FixMessage, tag};
    ///
    /// let mut m = FixMessage::new();
    /// m.set(tag::ORDER_QTY, 100).set(tag::ORDER_QTY, 250);
    /// assert_eq!(m.fields.len(), 1); // replaced in place
    /// assert_eq!(m.get(tag::ORDER_QTY), Some("250"));
    /// ```
    pub fn set(&mut self, tag: i32, value: impl ToString) -> &mut Self {
        let value = value.to_string();
        if let Some((_, v)) = self.fields.iter_mut().find(|(t, _)| *t == tag) {
            *v = value;
        } else {
            self.fields.push((tag, value));
        }
        self
    }

    /// First occurrence of a tag, if present.
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::{FixMessage, tag};
    ///
    /// let m = FixMessage::typed("D");
    /// assert_eq!(m.get(tag::SYMBOL), None);
    /// ```
    pub fn get(&self, tag: i32) -> Option<&str> {
        self.fields
            .iter()
            .find(|(t, _)| *t == tag)
            .map(|(_, v)| v.as_str())
    }

    /// Convenience accessor: the message's MsgType (35) value.
    ///
    /// Returns [`None`] for a message without tag 35 (the parser guarantees
    /// tag 35 somewhere on the frame, but [`FixMessage::new`] starts empty).
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::{FixMessage, msg_type};
    ///
    /// assert_eq!(FixMessage::new().msg_type(), None);
    /// assert_eq!(FixMessage::typed(msg_type::LOGON).msg_type(), Some(msg_type::LOGON));
    /// ```
    pub fn msg_type(&self) -> Option<&str> {
        self.get(tag::MSG_TYPE)
    }

    /// Parse a field value as [`i64`] (quantities, sequence numbers,
    /// numeric-tag values).
    ///
    /// Returns [`None`] when the tag is absent or its value is not a valid
    /// integer - callers decide whether absence is an error.
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::{FixMessage, tag};
    ///
    /// let mut m = FixMessage::new();
    /// m.set(tag::MSG_SEQ_NUM, "42");
    /// assert_eq!(m.as_i64(tag::MSG_SEQ_NUM), Some(42));
    /// assert_eq!(m.as_i64(tag::SYMBOL), None);
    /// ```
    pub fn as_i64(&self, tag_no: i32) -> Option<i64> {
        self.get(tag_no).and_then(|v| v.parse().ok())
    }

    /// Encode to a complete wire frame: `8=FIX.4.2|9=|body|10=nnn|`.
    ///
    /// Header contract: BeginString (8) first, computed BodyLength (9)
    /// second (body length **+ 7** for the `10=xxx`+SOH trailer), MsgType
    /// (35) hoisted to the front of the body, remaining fields in stored
    /// order, CheckSum (10) last (modulo-256 byte sum of everything before
    /// it, zero-padded to three digits). Tags 9 and 10 present in the field
    /// list are ignored - both are always recomputed from the body, so a
    /// stale cached value can never be written. A message without tag 35
    /// encodes with no MsgType field (the caller's contract).
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::{FixMessage, msg_type, tag};
    ///
    /// let mut hb = FixMessage::typed(msg_type::HEARTBEAT);
    /// hb.set(tag::MSG_SEQ_NUM, 1);
    /// let frame = hb.encode();
    /// assert!(frame.starts_with(b"8=FIX.4.2\x019="));
    /// assert!(frame.ends_with(&[0x01]));
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::new();
        // 35 first in the body
        if let Some(mt) = self.get(tag::MSG_TYPE) {
            body.extend_from_slice(format!("35={}{}", mt, SOH as char).as_bytes());
        }
        for (t, v) in &self.fields {
            if *t == tag::MSG_TYPE || *t == tag::BODY_LENGTH || *t == tag::CHECKSUM {
                continue;
            }
            body.extend_from_slice(format!("{}={}{}", t, v, SOH as char).as_bytes());
        }
        // BodyLength includes the trailer "10=xxx"+SOH = 7 bytes.
        let body_len = body.len() + 7;
        let mut frame = Vec::with_capacity(32 + body.len());
        frame.extend_from_slice(
            format!(
                "8={}{}9={}{}",
                FIX_42_BEGIN_STRING, SOH as char, body_len, SOH as char
            )
            .as_bytes(),
        );
        frame.extend_from_slice(&body);
        let ck = checksum(&frame);
        frame.extend_from_slice(format!("10={:03}{}", ck, SOH as char).as_bytes());
        frame
    }

    /// Parse a complete frame (`8=…10=nnn|SOH`).
    ///
    /// Validation order: BeginString (8) present and equal to
    /// [`FIX_42_BEGIN_STRING`], BodyLength (9) second and matching the
    /// computed length (which includes the CheckSum trailer), and CheckSum
    /// (10) matching the modulo-256 byte sum of everything before `10=`.
    /// Fields are collected in wire order; fields after tag 35 are stored
    /// verbatim (no schema check).
    ///
    /// # Errors
    /// Returns the [`FixError`] variants for structural violations: a short
    /// buffer that does not even start with `8=` is
    /// [`TooShort`](FixError::TooShort), a wrong/missing BeginString is
    /// [`NoBeginString`](FixError::NoBeginString), malformed fields are
    /// [`BadField`](FixError::BadField), and length / checksum mismatches
    /// are [`BodyLength`](FixError::BodyLength) /
    /// [`Checksum`](FixError::Checksum).
    ///
    /// # Examples
    /// ```
    /// use exchange8_fix::{FixMessage, msg_type};
    ///
    /// let frame = FixMessage::typed(msg_type::HEARTBEAT).encode();
    /// let parsed = FixMessage::parse(&frame).unwrap().unwrap();
    /// assert_eq!(parsed.msg_type(), Some(msg_type::HEARTBEAT));
    ///
    /// // Partial frame: keep buffering.
    /// let half = FixMessage::parse(&frame[..frame.len() / 2]).unwrap();
    /// assert!(half.is_none());
    /// ```
    pub fn parse(frame: &[u8]) -> Result<Option<FixMessage>, FixError> {
        if frame.len() < 16 {
            // Cannot possibly hold 8=FIX.4.2|9=nn|35=x|10=nnn|
            if frame.first() == Some(&b'8') {
                return Ok(None); // partial, keep reading
            }
            return Err(FixError::TooShort);
        }
        // Locate the checksum field "10=" at the end.
        let ck_start = match find_checksum_start(frame) {
            Some(p) => p,
            None => return Ok(None), // no terminating 10= yet
        };
        // Split into fields.
        let mut fields: Vec<(i32, String)> = Vec::new();
        let mut pos = 0usize;
        while pos < ck_start {
            let sep = match frame[pos..ck_start].iter().position(|&b| b == SOH) {
                Some(p) => pos + p,
                None => return Err(FixError::BadField(pos)),
            };
            let field = &frame[pos..sep];
            let eq = match field.iter().position(|&b| b == b'=') {
                Some(p) => p,
                None => return Err(FixError::BadField(pos)),
            };
            let tag_str = std::str::from_utf8(&field[..eq]).map_err(|_| FixError::BadField(pos))?;
            let tag_no: i32 = tag_str.parse().map_err(|_| FixError::BadField(pos))?;
            let val = String::from_utf8_lossy(&field[eq + 1..]).into_owned();
            fields.push((tag_no, val));
            pos = sep + 1;
        }
        let msg = FixMessage { fields };

        // Validate: first tag 8, second 9, third 35, and checksum.
        if msg.fields.is_empty() || msg.fields[0].0 != tag::BEGIN_STRING {
            return Err(FixError::NoBeginString);
        }
        if msg.fields[0].1 != FIX_42_BEGIN_STRING {
            return Err(FixError::NoBeginString);
        }
        if msg.fields.len() < 2 || msg.fields[1].0 != tag::BODY_LENGTH {
            return Err(FixError::BadField(1));
        }
        // BodyLength includes everything from after 9= to end incl. 10=.
        let body_start = frame
            .iter()
            .position(|&b| b == SOH)
            .map(|p| p + 1)
            .unwrap_or(0);
        let body_start = find_body_start(frame).unwrap_or(body_start);
        let declared: u32 = msg.fields[1].1.parse().map_err(|_| FixError::BadField(1))?;
        let computed = frame.len() - body_start;
        if declared as usize != computed {
            return Err(FixError::BodyLength {
                declared,
                computed: computed as u32,
            });
        }
        // Checksum: everything before "10=".
        let computed_ck = checksum(&frame[..ck_start]);
        let ck_field = &frame[ck_start..];
        // "10=nnn" + SOH
        let ck_val: u8 = std::str::from_utf8(&ck_field[3..ck_field.len() - 1])
            .ok()
            .and_then(|s| s.trim_end_matches(SOH as char).parse().ok())
            .unwrap_or(u8::MAX);
        if ck_val != computed_ck {
            return Err(FixError::Checksum {
                declared: ck_val,
                computed: computed_ck,
            });
        }
        Ok(Some(msg))
    }
}

fn find_body_start(frame: &[u8]) -> Option<usize> {
    // Body starts after the second SOH ("8=FIX.4.2|9=nnn|" → next byte).
    let mut soh_count = 0;
    for (i, &b) in frame.iter().enumerate() {
        if b == SOH {
            soh_count += 1;
            if soh_count == 2 {
                return Some(i + 1);
            }
        }
    }
    None
}

fn find_checksum_start(frame: &[u8]) -> Option<usize> {
    // Last occurrence of "10=" followed by 3 digits + SOH at the very end.
    if frame.len() < 7 {
        return None;
    }
    let tail = &frame[frame.len() - 7..];
    if tail.starts_with(b"10=")
        && tail[4].is_ascii_digit()
        && tail[5].is_ascii_digit()
        && tail[6].is_ascii_digit()
        && frame.ends_with(&[SOH])
        && frame.len() >= 8
        && frame[frame.len() - 1] == SOH
    {
        Some(frame.len() - 7)
    } else if frame.ends_with(&[SOH]) && frame.len() >= 8 {
        // fallback scan: last "10="
        let mut found = None;
        for i in 0..frame.len().saturating_sub(3) {
            if &frame[i..i + 3] == b"10=" && i + 7 <= frame.len() {
                found = Some(i);
            }
        }
        found
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(msg_type: &str, extra: &[(i32, &str)]) -> FixMessage {
        let mut m = FixMessage::new();
        m.set(tag::MSG_TYPE, msg_type);
        for (t, v) in extra {
            m.set(*t, *v);
        }
        m
    }

    #[test]
    fn encode_parse_round_trip() {
        let m = build(
            msg_type::NEW_ORDER,
            &[
                (tag::MSG_SEQ_NUM, "42"),
                (tag::SENDER_COMP_ID, "CLIENT1"),
                (tag::TARGET_COMP_ID, "EX8"),
                (tag::SYMBOL, "ACME"),
                (tag::SIDE, "1"),
                (tag::ORDER_QTY, "100"),
                (tag::ORD_TYPE, "2"),
                (tag::PRICE, "10.50"),
                (tag::TIME_IN_FORCE, "0"),
            ],
        );
        let frame = m.encode();
        let parsed = FixMessage::parse(&frame).unwrap().unwrap();
        assert_eq!(parsed.msg_type(), Some("D"));
        assert_eq!(parsed.get(tag::SYMBOL), Some("ACME"));
        assert_eq!(parsed.get(tag::PRICE), Some("10.50"));
        assert_eq!(parsed.get(tag::MSG_SEQ_NUM), Some("42"));
        assert_eq!(parsed.get(tag::SENDER_COMP_ID), Some("CLIENT1"));
    }

    #[test]
    fn partial_frame_returns_none() {
        let m = build(
            msg_type::LOGON,
            &[(tag::MSG_SEQ_NUM, "1"), (tag::HEART_BT_INT, "30")],
        );
        let frame = m.encode();
        let half = &frame[..frame.len() / 2];
        assert!(FixMessage::parse(half).unwrap().is_none());
    }

    #[test]
    fn corrupt_checksum_rejected() {
        let m = build(msg_type::LOGON, &[(tag::MSG_SEQ_NUM, "1")]);
        let mut frame = m.encode();
        // Flip a byte in the middle (not header/checksum).
        let mid = frame.len() / 2;
        frame[mid] ^= 0x01;
        let err = FixMessage::parse(&frame).unwrap_err();
        assert!(matches!(err, FixError::Checksum { .. }));
    }

    #[test]
    fn body_length_mismatch_rejected() {
        let m = build(msg_type::LOGON, &[(tag::MSG_SEQ_NUM, "1")]);
        let mut frame = m.encode();
        // Corrupt the declared body length: find "9=" and change the digit.
        let nine = frame.iter().position(|&b| b == b'9').unwrap();
        frame[nine + 2] = b'9';
        // Recompute checksum so only BodyLength fails.
        let ck_start = find_checksum_start(&frame).unwrap();
        let ck = checksum(&frame[..ck_start]);
        let digits = format!("{:03}", ck);
        let tail_start = frame.len() - 4;
        frame[tail_start] = digits.as_bytes()[0];
        frame[tail_start + 1] = digits.as_bytes()[1];
        frame[tail_start + 2] = digits.as_bytes()[2];
        let err = FixMessage::parse(&frame).unwrap_err();
        assert!(matches!(err, FixError::BodyLength { .. }));
    }

    #[test]
    fn custom_tags_round_trip() {
        let m = build(
            msg_type::NEW_ORDER,
            &[
                (tag::MSG_SEQ_NUM, "7"),
                (tag::BOOK_CODE, "XL8M"),
                (tag::ANONYMOUS, "N"),
                (tag::PROTECTION, "K"),
                (tag::ORDER_POST_ONLY, "N"),
            ],
        );
        let frame = m.encode();
        let parsed = FixMessage::parse(&frame).unwrap().unwrap();
        assert_eq!(parsed.get(tag::BOOK_CODE), Some("XL8M"));
        assert_eq!(parsed.get(tag::PROTECTION), Some("K"));
        assert_eq!(parsed.get(9730), None); // absent
    }

    #[test]
    fn fx_quote_request_round_trip() {
        // Client RFQ: buy 1M EURUSD spot.
        let m = FixMessage::quote_request("QR-17", "EUR/USD", "1", 1_000_000, "SP");
        let parsed = FixMessage::parse(&m.encode()).unwrap().unwrap();
        assert_eq!(parsed.msg_type(), Some("R"));
        assert_eq!(parsed.get(tag::QUOTE_REQ_ID), Some("QR-17"));
        assert_eq!(parsed.get(tag::SYMBOL), Some("EUR/USD"));
        assert_eq!(parsed.get(tag::SIDE), Some("1"));
        assert_eq!(parsed.get(tag::ORDER_QTY), Some("1000000"));
        assert_eq!(parsed.get(tag::TEXT), Some("SP"));
    }

    #[test]
    fn fx_quote_round_trip() {
        // Venue tradeable quote: two-way 1.08450/1.08456, T+2.
        let m = FixMessage::quote(
            "QR-17",
            "Q-88",
            "EUR/USD",
            "1.08450",
            "1.08456",
            1_000_000,
            "20250115-14:30:00.123",
            Some("20250117"),
        );
        let parsed = FixMessage::parse(&m.encode()).unwrap().unwrap();
        assert_eq!(parsed.msg_type(), Some("s"));
        assert_eq!(parsed.get(tag::QUOTE_REQ_ID), Some("QR-17"));
        assert_eq!(parsed.get(tag::QUOTE_ID), Some("Q-88"));
        assert_eq!(parsed.get(tag::BID_PX), Some("1.08450"));
        assert_eq!(parsed.get(tag::OFFER_PX), Some("1.08456"));
        assert_eq!(parsed.get(tag::QUOTE_TYPE), Some("1"));
        assert_eq!(
            parsed.get(tag::VALID_UNTIL_TIME),
            Some("20250115-14:30:00.123")
        );
        assert_eq!(parsed.get(tag::SETTL_DATE), Some("20250117"));
    }

    #[test]
    fn fx_quote_response_and_status_round_trip() {
        // Client hits the quote.
        let hit = FixMessage::quote_accept("QR-17", "Q-88");
        let parsed = FixMessage::parse(&hit.encode()).unwrap().unwrap();
        assert_eq!(parsed.msg_type(), Some("aj"));
        assert_eq!(parsed.get(tag::QUOTE_RESP_TYPE), Some("0"));
        // Venue confirms traded (0) / rejected on last look (7).
        let ok = FixMessage::quote_status("QR-17", "Q-88", "0", None);
        let parsed = FixMessage::parse(&ok.encode()).unwrap().unwrap();
        assert_eq!(parsed.msg_type(), Some("AJ"));
        assert_eq!(parsed.get(tag::QUOTE_STATUS), Some("0"));
        let rej = FixMessage::quote_status("QR-17", "Q-88", "7", Some("last-look"));
        let parsed = FixMessage::parse(&rej.encode()).unwrap().unwrap();
        assert_eq!(parsed.get(tag::TEXT), Some("last-look"));
    }
}
