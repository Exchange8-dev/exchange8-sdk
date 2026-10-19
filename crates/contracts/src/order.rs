// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Normalized order/instrument DTOs (FIX 4.2 superset, per-instrument
//! precision).
//!
//! The gateway's layered wire structs (`AppMsgData → FixOrder →
//! analysisOrder`) collapse into one normalized type here, consumed
//! unchanged by Risk and the matching engine.
//!
//! Design: `crates/contracts/DESIGN.md` (module inventory §3),
//! `design/sentin8-exchange8-gateway-technical-design.md` §4.3.

use exchange8_fixed::{Price, Qty};
use serde::{Deserialize, Serialize};

use crate::enums::{Capacity, PriceProtection, Side, Visibility};

/// Reference to a tradable instrument.
///
/// Carries the identity and precision contract of one instrument; orders
/// embed it so validation can compare an order's price scale against the
/// instrument's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentRef {
    /// Internal numeric instrument id (ME shard key).
    pub id: u32,
    /// Trading symbol (e.g. `ACME`).
    pub symbol: String,
    /// ISIN if allocated.
    pub isin: Option<String>,
    /// ISO 10383 MIC (4 chars).
    pub mic: String,
    /// Settlement currency (ISO 4217).
    pub currency: String,
    /// Price decimal digits for this instrument (per-instrument precision:
    /// 4 equity, 8 crypto; see `DESIGN.md` §5).
    pub price_scale: u8,
    /// Quantity decimal digits (0 equity board lots, 8 crypto).
    pub qty_scale: u8,
    /// Board lot size; odd lots are multiples below this (scale 0 only).
    pub lot_size: Qty,
    /// Exchange8 instrument type.
    ///
    /// Serialized as `"type"` (the Rust keyword is renamed in JSON).
    #[serde(rename = "type")]
    pub kind: InstrumentKind,
}

/// Exchange8 instrument kind (equity / crypto / FX / derivative contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstrumentKind {
    /// Common/preferred equity share.
    Equity,
    /// Fixed-income security (bond).
    FixedIncome,
    /// Digital-asset instrument (8-digit price/qty scales).
    Crypto,
    /// Foreign-exchange pair.
    Fx,
    /// Listed futures contract.
    Future,
    /// Perpetual (non-expiring) futures contract.
    Perpetual,
    /// Listed option contract.
    Option,
}

/// Consolidated-book membership token.
///
/// A single character identifying which book family an instrument/order
/// prints on (lit vs hidden books of the feed topology).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BookToken(
    /// The catalog book id char (`L` lit, `D` dark pool, `O`/`Q` others).
    pub char,
);

impl BookToken {
    /// True when the instrument prints on the lit book.
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::BookToken;
    /// assert!(BookToken('L').is_lit());
    /// assert!(!BookToken('D').is_lit());
    /// ```
    pub fn is_lit(self) -> bool {
        self.0 == 'M' || self.0 == 'L'
    }
}

/// Order side. FIX 54 codes preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    /// `'1'` - buy.
    Buy,
    /// `'2'` - sell (unmarked).
    Sell,
    /// `'5'` - short sell (locate assumed upstream).
    ShortSell,
    /// `'6'` - short sale exempt.
    ShortExempt,
    /// `'8'` - cross (both sides on this symbol).
    Cross,
    /// `'9'` - cross short leg.
    CrossShort,
}

impl OrderSide {
    /// FIX 54 single-character code.
    pub const fn as_char(self) -> char {
        match self {
            OrderSide::Buy => '1',
            OrderSide::Sell => '2',
            OrderSide::ShortSell => '5',
            OrderSide::ShortExempt => '6',
            OrderSide::Cross => '8',
            OrderSide::CrossShort => '9',
        }
    }

    /// Interpret a FIX 54 code.
    ///
    /// Returns `None` for any code outside the FIX 54 side set
    /// (`"1"`, `"2"`, `"5"`, `"6"`, `"8"`, `"9"`).
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::OrderSide;
    /// assert_eq!(OrderSide::from_fix("1"), Some(OrderSide::Buy));
    /// assert_eq!(OrderSide::from_fix("7"), None);
    /// ```
    pub fn from_fix(code: &str) -> Option<Self> {
        match code {
            "1" => Some(OrderSide::Buy),
            "2" => Some(OrderSide::Sell),
            "5" => Some(OrderSide::ShortSell),
            "6" => Some(OrderSide::ShortExempt),
            "8" => Some(OrderSide::Cross),
            "9" => Some(OrderSide::CrossShort),
            _ => None,
        }
    }

    /// ME side (crosses collapse to the aggressor side at the GW).
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::OrderSide;
    /// assert_eq!(OrderSide::Cross.me_side(),
    ///     exchange8_contracts::enums::Side::Bid);
    /// ```
    pub const fn me_side(self) -> Side {
        match self {
            OrderSide::Buy | OrderSide::Cross => Side::Bid,
            _ => Side::Ask,
        }
    }

    /// True when the side is `ShortSell` or `ShortExempt`.
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::OrderSide;
    /// assert!(OrderSide::ShortSell.is_short());
    /// assert!(!OrderSide::Buy.is_short());
    /// ```
    pub const fn is_short(self) -> bool {
        matches!(self, OrderSide::ShortSell | OrderSide::ShortExempt)
    }

    /// True when the side is `Cross` or `CrossShort`.
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::OrderSide;
    /// assert!(OrderSide::Cross.is_cross());
    /// assert!(!OrderSide::Sell.is_cross());
    /// ```
    pub const fn is_cross(self) -> bool {
        matches!(self, OrderSide::Cross | OrderSide::CrossShort)
    }
}

/// Time in force (FIX 59).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tif {
    /// `'0'` - valid for the session only (default).
    Day,
    /// `'1'` - good till canceled.
    Gtc,
    /// `'2'` - execute at the opening auction, cancel otherwise.
    AtOpen,
    /// `'3'` - immediate-or-cancel.
    Ioc,
    /// `'4'` - fill-or-kill.
    Fok,
    /// `'5'` - valid for the crossing session only.
    Crossing,
    /// `'6'` - good till date.
    Gtd,
}

impl Tif {
    /// FIX 59 single-character code.
    pub const fn as_char(self) -> char {
        match self {
            Tif::Day => '0',
            Tif::Gtc => '1',
            Tif::AtOpen => '2',
            Tif::Ioc => '3',
            Tif::Fok => '4',
            Tif::Crossing => '5',
            Tif::Gtd => '6',
        }
    }

    /// Interpret a FIX 59 code (empty = Day).
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::Tif;
    /// assert_eq!(Tif::from_fix(None), Some(Tif::Day));
    /// assert_eq!(Tif::from_fix(Some("4")), Some(Tif::Fok));
    /// ```
    pub fn from_fix(code: Option<&str>) -> Option<Self> {
        match code {
            None | Some("") | Some("0") => Some(Tif::Day),
            Some("1") => Some(Tif::Gtc),
            Some("2") => Some(Tif::AtOpen),
            Some("3") => Some(Tif::Ioc),
            Some("4") => Some(Tif::Fok),
            Some("5") => Some(Tif::Crossing),
            Some("6") => Some(Tif::Gtd),
            _ => None,
        }
    }
}

/// FIX 40 OrdType (exchange8 subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrdType {
    /// `"2"` - limit at the given price.
    Limit,
    /// `"P"` - pegged to a reference price + peg difference.
    Pegged,
}

impl OrdType {
    /// FIX 40 code (`"2"` limit, `"P"` pegged).
    pub const fn as_char(self) -> char {
        match self {
            OrdType::Limit => '2',
            OrdType::Pegged => 'P',
        }
    }

    /// Interpret a FIX 40 code.
    ///
    /// Returns `None` for any code outside the exchange8 subset
    /// (`"2"` limit, `"P"` pegged).
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::OrdType;
    /// assert_eq!(OrdType::from_fix("P"), Some(OrdType::Pegged));
    /// assert_eq!(OrdType::from_fix("1"), None);
    /// ```
    pub fn from_fix(code: &str) -> Option<Self> {
        match code {
            "2" => Some(OrdType::Limit),
            "P" => Some(OrdType::Pegged),
            _ => None,
        }
    }
}

/// Peg type (FIX ExecInst 18 chars: M/P/R/D).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PegType {
    /// `'M'` - midpoint.
    Midpoint,
    /// `'P'` - market peg (join opposite side).
    Market,
    /// `'R'` - primary peg (join own side).
    Primary,
    /// `'D'` - darkpool (midpoint, always hidden).
    Darkpool,
}

impl PegType {
    /// FIX ExecInst peg character (`M`/`P`/`R`/`D`).
    pub const fn as_char(self) -> char {
        match self {
            PegType::Midpoint => 'M',
            PegType::Market => 'P',
            PegType::Primary => 'R',
            PegType::Darkpool => 'D',
        }
    }

    /// Interpret a FIX ExecInst peg character.
    ///
    /// Returns `None` for any char outside `M`/`P`/`R`/`D`.
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::PegType;
    /// assert_eq!(PegType::from_char('M'), Some(PegType::Midpoint));
    /// assert_eq!(PegType::from_char('X'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'M' => Some(PegType::Midpoint),
            'P' => Some(PegType::Market),
            'R' => Some(PegType::Primary),
            'D' => Some(PegType::Darkpool),
            _ => None,
        }
    }
}

/// Self-trade prevention action - explicit per-order instead of a
/// signed-counterparty identifier.
///
/// Determines what happens to both sides when an order's STP key matches
/// the resting side of a potential trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StpAction {
    /// Kill the incoming (active) order on self-trade.
    KillActive,
    /// Kill the resting (passive) order on self-trade.
    KillPassive,
    /// Kill both.
    KillBoth,
    /// Reduce both by min(remaining, resting).
    Decrement,
}

/// STP key type (what the match compares).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StpKeyType {
    /// Compare self-trade across orders of the same customer.
    Customer,
    /// Compare self-trade across orders from the same trading location.
    Location,
    /// Compare self-trade across orders on the same gateway session.
    Connection,
    /// Compare self-trade across orders on the same account.
    Account,
}

/// The normalized gateway order - the single collapsed order type produced
/// by session decoding and consumed by Risk and the matching engine.
///
/// Lifecycle fields (`ord_status`, `cum_qty`, `leaves_qty`, `avg_px`) are
/// advanced together by producers as lifecycle events land.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeOrder {
    // Identity
    /// Tenant environment of the order (globally unique).
    pub env_id: String,
    /// Gateway-assigned order id (routing key inside the vertical).
    pub gw_order_id: u64,
    /// Client-facing order reference (FIX 11).
    pub cl_ord_id: String,
    /// Original client ref when this order replaces an earlier one (FIX 41).
    pub orig_cl_ord_id: Option<String>,
    /// Client account the order belongs to (FIX 1).
    pub account: String,
    /// Client FIX sender company id (FIX 49).
    pub client_comp_id: String,
    /// Original firm for a given-away order (FIX 116), if any.
    pub on_behalf_comp_id: Option<String>,
    /// Submitting user id, when the session carries one (FIX 553).
    pub user_id: Option<String>,
    /// Gateway session that submitted the order (Risk/ME routing key).
    pub connection_id: u16,

    // Instrument
    /// Resolved instrument (symbol, MIC, currency, precision, lot size).
    pub instrument: InstrumentRef,

    // Order attributes
    /// Order side (FIX 54 superset; crosses collapse at the gateway).
    pub side: OrderSide,
    /// Total ordered quantity (scaled).
    pub qty: Qty,
    /// Minimum execution quantity when the client demanded one (FIX 110).
    pub min_qty: Option<Qty>,
    /// Limit price; `None` only when the order type carries no price
    /// (market-like pegs).
    pub price: Option<Price>,
    /// FIX 40 order type (limit or pegged).
    pub ord_type: OrdType,
    /// Time in force (FIX 59).
    pub tif: Tif,
    /// FIX 18 peg chars (subset) + post-only bid/offer + aon + bypass.
    pub peg_type: Option<PegType>,
    /// Peg offset from the reference price, in the instrument's price scale.
    pub peg_difference: Option<i128>,
    /// Peg the consolidated EBBO instead of own-book prices.
    pub peg_ebbo: bool,
    /// Displayed vs hidden quantity handling.
    pub visibility: Visibility,
    /// Iceberg display floor (FIX 111), when the order is an iceberg.
    pub max_floor: Option<Qty>,
    /// Agency vs principal capacity (FIX 528).
    pub capacity: Capacity,
    /// Publish order details anonymously on feeds.
    pub anonymous: bool,
    /// IOC matching only visible levels (hidden ignored).
    pub bypass: bool,
    /// Price-protection request (kill on violation, or sidestep-then-post).
    pub price_protection: Option<PriceProtection>,
    /// Never match; kill if the order would cross (post-only marker).
    pub post_only: bool,
    /// Primary book token (lit/hidden family) of the order.
    pub book: BookToken,
    /// Strategy-book tokens (`*` = all books) the order participates in.
    pub strategy: Vec<BookToken>,
    /// Self-trade-prevention key: what is compared and its value.
    pub stp_key: Option<(StpKeyType, u64)>,
    /// Self-trade-prevention action taken on a violation.
    pub stp_action: Option<StpAction>,
    /// Cross-order extension data, when the order is a cross.
    pub cross: Option<CrossData>,
    /// Requested destination market (FIX 100), when set.
    pub ex_destination: Option<String>,

    // State (mutated by lifecycle events)
    /// FIX 39 execution status (the lifecycle state of this order).
    pub ord_status: OrdStatus,
    /// Executed quantity so far (scaled).
    pub cum_qty: Qty,
    /// Open quantity (scaled); zero on terminal statuses.
    pub leaves_qty: Qty,
    /// Average execution price, once partially or fully filled.
    pub avg_px: Option<Price>,
    /// Order's latest update timestamp, microseconds since the Unix epoch.
    pub timestamp_us: u64,
}

/// Cross order extension (FIX 380 cross-order fields).
///
/// Carries the obligor details of a cross order for regulatory records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossData {
    /// Cross type (FIX 54 subtypes): `B` cross, `C` cross-SR, `I` cross
    /// internalization, `M` matching, `V` give-up.
    pub cross_type: char,
    /// Executing party account, when known.
    pub account: Option<String>,
    /// Obligor account type id, when known.
    pub account_type_id: Option<String>,
    /// Regulation reference (e.g. § 11a2), when known.
    pub regulation_id: Option<String>,
    /// Jitney (carry) broker id, when known.
    pub jitney: Option<String>,
}

impl ExchangeOrder {
    /// Risk-state lookup key (session that owns the order).
    ///
    /// Every order of one gateway session shares the same key, so Risk can
    /// aggregate and release reservations per session.
    ///
    /// # Examples
    /// ```
    /// # use exchange8_contracts::{ExchangeOrder, BookToken};
    /// # // Constructing an `ExchangeOrder` fixture field-by-field is
    /// # // verbose; see the gateway tests (`submission::order`) for a
    /// # // real builder. Reads are trivial:
    /// # struct Demo { connection_id: u16 }
    /// let demo = Demo { connection_id: 7 };
    /// assert_eq!(demo.connection_id, 7_u16);
    /// ```
    ///
    /// # Panics
    /// Never panics - a `const` field read.
    pub const fn connection_id(&self) -> u16 {
        self.connection_id
    }

    /// Reject code when the instrument cannot be resolved - the gateway
    /// resolved a symbol but Risk's snapshot disagrees (stale cache or an
    /// unknown id).
    ///
    /// # Errors
    /// Not an error-returning function; the reject code is the mapped value
    /// the caller converts into a reject path.
    pub fn unresolved_instrument_code(&self) -> crate::reject::RejectCode {
        use crate::reject::RejectCode;
        if self.instrument.symbol.is_empty() {
            RejectCode::InvalidSymbol
        } else {
            RejectCode::InvalidSecurityId
        }
    }
}

/// FIX 39 OrdStatus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrdStatus {
    /// `'A'`
    PendingNew,
    /// `'0'`
    New,
    /// `'1'`
    PartialFill,
    /// `'2'`
    Fill,
    /// `'3'`
    DoneForDay,
    /// `'4'`
    Canceled,
    /// `'5'`
    Replaced,
    /// `'6'`
    PendingCancel,
    /// `'8'`
    Rejected,
    /// `'9'`
    Suspended,
    /// `'C'`
    Expired,
    /// `'E'`
    PendingReplace,
}

impl OrdStatus {
    /// FIX 39 execution-status character.
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::OrdStatus;
    /// assert_eq!(OrdStatus::PendingNew.as_char(), 'A');
    /// assert_eq!(OrdStatus::Canceled.as_char(), '4');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            OrdStatus::PendingNew => 'A',
            OrdStatus::New => '0',
            OrdStatus::PartialFill => '1',
            OrdStatus::Fill => '2',
            OrdStatus::DoneForDay => '3',
            OrdStatus::Canceled => '4',
            OrdStatus::Replaced => '5',
            OrdStatus::PendingCancel => '6',
            OrdStatus::Rejected => '8',
            OrdStatus::Suspended => '9',
            OrdStatus::Expired => 'C',
            OrdStatus::PendingReplace => 'E',
        }
    }

    /// True when the status terminates the order lifecycle.
    ///
    /// Terminal states are `Fill`, `Canceled`, `Rejected`, and `Expired`.
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::OrdStatus;
    /// assert!(OrdStatus::Fill.is_terminal());
    /// assert!(!OrdStatus::New.is_terminal());
    /// ```
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            OrdStatus::Fill | OrdStatus::Canceled | OrdStatus::Rejected | OrdStatus::Expired
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FIX 54 side codes round-trip exactly (unknown codes rejected).
    #[test]
    fn fix54_side_codes() {
        assert_eq!(OrderSide::from_fix("1"), Some(OrderSide::Buy));
        assert_eq!(OrderSide::from_fix("2"), Some(OrderSide::Sell));
        assert_eq!(OrderSide::from_fix("5"), Some(OrderSide::ShortSell));
        assert_eq!(OrderSide::from_fix("8"), Some(OrderSide::Cross));
        assert_eq!(OrderSide::from_fix("7"), None);
        assert_eq!(OrderSide::Buy.as_char(), '1');
        assert_eq!(OrderSide::Cross.me_side(), Side::Bid);
        assert!(OrderSide::ShortSell.is_short());
        assert!(OrderSide::Cross.is_cross());
    }

    /// FIX 59 TIF codes round-trip; empty/omitted = Day.
    #[test]
    fn fix59_tif_codes() {
        assert_eq!(Tif::from_fix(None), Some(Tif::Day));
        assert_eq!(Tif::from_fix(Some("")), Some(Tif::Day));
        assert_eq!(Tif::from_fix(Some("3")), Some(Tif::Ioc));
        assert_eq!(Tif::from_fix(Some("4")), Some(Tif::Fok));
        assert_eq!(Tif::from_fix(Some("9")), None);
    }

    /// Terminal-status set matches the FIX 39 terminal states.
    #[test]
    fn ord_status_terminal() {
        assert!(OrdStatus::Fill.is_terminal());
        assert!(OrdStatus::Canceled.is_terminal());
        assert!(!OrdStatus::New.is_terminal());
        assert!(!OrdStatus::PartialFill.is_terminal());
    }

    /// Peg ExecInst chars round-trip; unknown chars rejected.
    #[test]
    fn peg_chars() {
        assert_eq!(PegType::from_char('M'), Some(PegType::Midpoint));
        assert_eq!(PegType::from_char('D'), Some(PegType::Darkpool));
        assert_eq!(PegType::from_char('X'), None);
    }
}
