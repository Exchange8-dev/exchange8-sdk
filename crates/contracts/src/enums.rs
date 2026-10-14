// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Shared char-code enums — 1:1 with the matching-engine wire constants.
//!
//! Every enum here pairs Rust variants with the exact single-character codes
//! of the matching-engine protocol; the unit tests iterate the full tables
//! against the extracted behavior catalog. Consumers: the ME engine/shard,
//! the gateway's OUCH/session layers, MD, and Clearing all import from this
//! module rather than re-declaring wire constants.
//!
//! Truth: `design/behavior-catalog/messages.json` §`me_enums`
//! (extracted from the protocol header constants: order types, sides,
//! visibility, capacity, statuses, kill reasons, reject reasons, liquidity,
//! lot types, system events).
//!
//! Design: `crates/contracts/DESIGN.md` §3 (data model), §4 (API surface).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ME order type — the catalog `order_type` table (`MEMsgs.h:32-36`)
// ---------------------------------------------------------------------------

/// ME order type — how the engine may match and what happens to the
/// remainder.
///
/// Encodes the catalog `order_type` table (`L`/`F`/`I`/`P`/`B`):
/// limit orders post the unmatched remainder; IOC/FOK orders do not rest;
/// post-only orders would rather not trade; bypass orders ignore hidden
/// levels while matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    /// `'L'` — match then post remainder.
    Limit,
    /// `'F'` — fill-or-kill; no partial.
    Fok,
    /// `'I'` — immediate-or-cancel; remainder killed.
    Ioc,
    /// `'P'` — never matches; kill if it would cross.
    PostOnly,
    /// `'B'` — IOC matching only **visible** levels (hidden ignored).
    Bypass,
}

impl OrderType {
    /// Wire char (`L`/`F`/`I`/`P`/`B`).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::OrderType;
    /// assert_eq!(OrderType::Limit.as_char(), 'L');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            OrderType::Limit => 'L',
            OrderType::Fok => 'F',
            OrderType::Ioc => 'I',
            OrderType::PostOnly => 'P',
            OrderType::Bypass => 'B',
        }
    }

    /// Interpret the ME order-type wire char.
    ///
    /// Returns `None` for any char outside `L`/`F`/`I`/`P`/`B`.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::OrderType;
    /// assert_eq!(OrderType::from_char('B'), Some(OrderType::Bypass));
    /// assert_eq!(OrderType::from_char('x'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'L' => Some(OrderType::Limit),
            'F' => Some(OrderType::Fok),
            'I' => Some(OrderType::Ioc),
            'P' => Some(OrderType::PostOnly),
            'B' => Some(OrderType::Bypass),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ME book side — the catalog `side` table (`MEMsgs.h:27-28`)
// ---------------------------------------------------------------------------

/// ME book side — which side of the internal book the order joins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    /// `'B'`
    Bid,
    /// `'A'`
    Ask,
}

impl Side {
    /// ME wire char (`B` bid / `A` ask).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Side;
    /// assert_eq!(Side::Bid.as_char(), 'B');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            Side::Bid => 'B',
            Side::Ask => 'A',
        }
    }

    /// Interpret the ME side wire char.
    ///
    /// Returns `None` for any char outside `B`/`A`.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Side;
    /// assert_eq!(Side::from_char('A'), Some(Side::Ask));
    /// assert_eq!(Side::from_char('1'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'B' => Some(Side::Bid),
            'A' => Some(Side::Ask),
            _ => None,
        }
    }

    /// Opposite side of the book.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Side;
    /// assert_eq!(Side::Bid.opposite(), Side::Ask);
    /// ```
    pub const fn opposite(self) -> Side {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }
}

// ---------------------------------------------------------------------------
// Visibility — the catalog `visible` table (`MEMsgs.h:38-39`)
// ---------------------------------------------------------------------------

/// Visible vs hidden quantity handling on the ME book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    /// `'D'` — prints on the lit feed.
    Display,
    /// `'H'` — hidden orders are not sent to the ITCH feed; they live in
    /// separate hidden price levels (bypass only matches visible).
    Hidden,
}

impl Visibility {
    /// ME wire char (`D` display / `H` hidden).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Visibility;
    /// assert_eq!(Visibility::Hidden.as_char(), 'H');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            Visibility::Display => 'D',
            Visibility::Hidden => 'H',
        }
    }

    /// Interpret the visibility wire char.
    ///
    /// Returns `None` for any char outside `D`/`H`.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Visibility;
    /// assert_eq!(Visibility::from_char('D'), Some(Visibility::Display));
    /// assert_eq!(Visibility::from_char('X'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'D' => Some(Visibility::Display),
            'H' => Some(Visibility::Hidden),
            _ => None,
        }
    }

    /// True when the visibility prints on the lit feed.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Visibility;
    /// assert!(Visibility::Display.is_visible());
    /// assert!(!Visibility::Hidden.is_visible());
    /// ```
    pub const fn is_visible(self) -> bool {
        matches!(self, Visibility::Display)
    }
}

// ---------------------------------------------------------------------------
// Capacity — the catalog `capacity` table (`MEMsgs.h:44-45`)
// ---------------------------------------------------------------------------

/// Order capacity — agency (client order) vs principal (own-house order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capacity {
    /// `'A'` — acting as agent, on behalf of a customer.
    Agency,
    /// `'P'` — acting as principal (firm's own account).
    Principal,
}

impl Capacity {
    /// ME wire char (`A` agency / `P` principal).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Capacity;
    /// assert_eq!(Capacity::Principal.as_char(), 'P');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            Capacity::Agency => 'A',
            Capacity::Principal => 'P',
        }
    }

    /// Interpret the capacity wire char.
    ///
    /// Returns `None` for any char outside `A`/`P`.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Capacity;
    /// assert_eq!(Capacity::from_char('A'), Some(Capacity::Agency));
    /// assert_eq!(Capacity::from_char('P'), Some(Capacity::Principal));
    /// assert_eq!(Capacity::from_char('C'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'A' => Some(Capacity::Agency),
            'P' => Some(Capacity::Principal),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Price protection — the catalog `price_protection` table
// ---------------------------------------------------------------------------

/// Price-protection behavior when a marketable order breaches the
/// protection band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PriceProtection {
    /// `'K'` — kill when the protection band is hit.
    Kill,
    /// `'P'` — sidestep one tick then post.
    Post,
}

impl PriceProtection {
    /// ME wire char (`K` kill / `P` post).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::PriceProtection;
    /// assert_eq!(PriceProtection::Kill.as_char(), 'K');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            PriceProtection::Kill => 'K',
            PriceProtection::Post => 'P',
        }
    }

    /// Interpret the price-protection wire char.
    ///
    /// Returns `None` for any char outside `K`/`P`.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::PriceProtection;
    /// assert_eq!(PriceProtection::from_char('P'),
    ///     Some(PriceProtection::Post));
    /// assert_eq!(PriceProtection::from_char('Z'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'K' => Some(PriceProtection::Kill),
            'P' => Some(PriceProtection::Post),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Instrument status — the catalog `instrument_status` table
// ---------------------------------------------------------------------------

/// Per-instrument trading state (catalog `instrument_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InstrumentStatus {
    /// `'O'` — open and matching.
    Open,
    /// `'H'` — halt kills all resting orders then rejects new.
    Halted,
    /// `'Q'` — plain display limit orders only, no matching.
    QuotationOnly,
}

impl InstrumentStatus {
    /// Feed wire char (`O` open / `H` halted / `Q` quotation only).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::InstrumentStatus;
    /// assert_eq!(InstrumentStatus::QuotationOnly.as_char(), 'Q');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            InstrumentStatus::Open => 'O',
            InstrumentStatus::Halted => 'H',
            InstrumentStatus::QuotationOnly => 'Q',
        }
    }

    /// Interpret the instrument-status wire char.
    ///
    /// Returns `None` for any char outside `O`/`H`/`Q`.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::InstrumentStatus;
    /// assert_eq!(InstrumentStatus::from_char('H'),
    ///     Some(InstrumentStatus::Halted));
    /// assert_eq!(InstrumentStatus::from_char('S'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'O' => Some(InstrumentStatus::Open),
            'H' => Some(InstrumentStatus::Halted),
            'Q' => Some(InstrumentStatus::QuotationOnly),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Market status — the catalog `market_status` table (the identical code
// set is shared by the ME and feed messages)
// ---------------------------------------------------------------------------

/// Per-book market state (catalog `market_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketStatus {
    /// `'O'` — reopen the book (accept + match orders again).
    Reopen,
    /// `'H'` — halts the book's orders.
    Halt,
    /// `'P'` — postpone trading (defer opening until further notice).
    Postpone,
    /// `'S'` — shut down the book for the session.
    Shutdown,
    /// `'D'` — postpone then shut down the book.
    PostponeShutdown,
}

impl MarketStatus {
    /// Feed wire char (`O`/`H`/`P`/`S`/`D`).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::MarketStatus;
    /// assert_eq!(MarketStatus::PostponeShutdown.as_char(), 'D');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            MarketStatus::Reopen => 'O',
            MarketStatus::Halt => 'H',
            MarketStatus::Postpone => 'P',
            MarketStatus::Shutdown => 'S',
            MarketStatus::PostponeShutdown => 'D',
        }
    }

    /// Interpret the market-status wire char.
    ///
    /// Returns `None` for any char outside `O`/`H`/`P`/`S`/`D`.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::MarketStatus;
    /// assert_eq!(MarketStatus::from_char('S'), Some(MarketStatus::Shutdown));
    /// assert_eq!(MarketStatus::from_char('Q'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'O' => Some(MarketStatus::Reopen),
            'H' => Some(MarketStatus::Halt),
            'P' => Some(MarketStatus::Postpone),
            'S' => Some(MarketStatus::Shutdown),
            'D' => Some(MarketStatus::PostponeShutdown),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Kill reason — the catalog `kill_reason` table
// (`order_cancel::Reason`)
// ---------------------------------------------------------------------------

/// Why a resting order left the book (catalog `kill_reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KillReason {
    /// `'H'` instrument/market halted
    InstrumentHalted,
    /// `'C'` client cancel request
    ClientRequest,
    /// `'A'` admin cancel
    AdminRequest,
    /// `'I'` IOC remainder
    Ioc,
    /// `'F'` FOK not fillable
    Fok,
    /// `'S'` self-trade prevention
    SelfTrade,
    /// `'R'` post-only would cross
    CrossMarket,
    /// `'P'` price protection kill
    PriceProtection,
    /// `'M'` modify/replace
    ModifyRequest,
    /// `'E'` external (routing) reject
    ExternalReject,
}

impl KillReason {
    /// Kill wire char (catalog `kill_reason` table).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::KillReason;
    /// assert_eq!(KillReason::SelfTrade.as_char(), 'S');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            KillReason::InstrumentHalted => 'H',
            KillReason::ClientRequest => 'C',
            KillReason::AdminRequest => 'A',
            KillReason::Ioc => 'I',
            KillReason::Fok => 'F',
            KillReason::SelfTrade => 'S',
            KillReason::CrossMarket => 'R',
            KillReason::PriceProtection => 'P',
            KillReason::ModifyRequest => 'M',
            KillReason::ExternalReject => 'E',
        }
    }

    /// Interpret the kill-reason wire char.
    ///
    /// Returns `None` for any char outside the catalog's
    /// `kill_reason` table (`H`/`C`/`A`/`I`/`F`/`S`/`R`/`P`/`M`/`E`).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::KillReason;
    /// assert_eq!(KillReason::from_char('I'), Some(KillReason::Ioc));
    /// assert_eq!(KillReason::from_char('X'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'H' => Some(KillReason::InstrumentHalted),
            'C' => Some(KillReason::ClientRequest),
            'A' => Some(KillReason::AdminRequest),
            'I' => Some(KillReason::Ioc),
            'F' => Some(KillReason::Fok),
            'S' => Some(KillReason::SelfTrade),
            'R' => Some(KillReason::CrossMarket),
            'P' => Some(KillReason::PriceProtection),
            'M' => Some(KillReason::ModifyRequest),
            'E' => Some(KillReason::ExternalReject),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ME transaction-reject reason — the catalog `reject_reason` table
// ---------------------------------------------------------------------------

/// ME transaction-reject reason (catalog `reject_reason`).
///
/// Carried on `TransactionRejected` to say why the operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MeRejectReason {
    /// `'H'` — the instrument is halted; no matching.
    InstrumentHalted,
    /// `'F'` — order id not found on the book.
    OrderNotFound,
    /// `'W'` — wrong originator (session does not own the order).
    WrongOriginator,
    /// `'U'` — unspecified failure at the engine.
    Unknown,
    /// `'E'` — an external (downstream) reject.
    ExternalReject,
    /// `'V'` — compliance/violation-based reject.
    Compliance,
}

impl MeRejectReason {
    /// ME transaction-reject wire char
    /// (`H`/`F`/`W`/`U`/`E`/`V`).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::MeRejectReason;
    /// assert_eq!(MeRejectReason::Compliance.as_char(), 'V');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            MeRejectReason::InstrumentHalted => 'H',
            MeRejectReason::OrderNotFound => 'F',
            MeRejectReason::WrongOriginator => 'W',
            MeRejectReason::Unknown => 'U',
            MeRejectReason::ExternalReject => 'E',
            MeRejectReason::Compliance => 'V',
        }
    }
}

// ---------------------------------------------------------------------------
// Liquidity / lot type — the catalog `liquidity` / `lot_type` tables
// ---------------------------------------------------------------------------

/// Liquidity role in an execution — added (passive) vs removed (aggressor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Liquidity {
    /// `'A'` — the resting order (passive).
    Added,
    /// `'R'` — the aggressor.
    Removed,
}

impl Liquidity {
    /// Execution wire char (`A` added / `R` removed).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::Liquidity;
    /// assert_eq!(Liquidity::Added.as_char(), 'A');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            Liquidity::Added => 'A',
            Liquidity::Removed => 'R',
        }
    }
}

/// Lot classification of an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LotType {
    /// `'1'` — below the board lot (odd lot).
    Odd,
    /// `'2'` — a multiple of the board lot.
    Board,
}

impl LotType {
    /// Execution wire char (`1` odd / `2` board).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::LotType;
    /// assert_eq!(LotType::Board.as_char(), '2');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            LotType::Odd => '1',
            LotType::Board => '2',
        }
    }
}

// ---------------------------------------------------------------------------
// System event codes — the ITCH `S` message payload (`FeedMsgs.h`)
// ---------------------------------------------------------------------------

/// ITCH system event codes — session lifecycle markers on the feed
/// (catalog `feed_messages` `S` message payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SystemEventCode {
    /// `'O'` — the first message of the feed session.
    StartOfMessages,
    /// `'C'` — the last message of the feed session.
    EndOfMessages,
    /// `'Q'` — market-hours trading begins.
    StartOfMarketHours,
    /// `'M'` — market-hours trading ends.
    EndOfMarketHours,
    /// `'S'` — system-hours trading begins.
    StartOfSystemHours,
    /// `'E'` — system-hours trading ends.
    EndOfSystemHours,
    /// `'P'` — the market enters the pre-open period.
    MarketPreopen,
}

impl SystemEventCode {
    /// ITCH `S` payload char (`O`/`C`/`Q`/`M`/`S`/`E`/`P`).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::SystemEventCode;
    /// assert_eq!(SystemEventCode::MarketPreopen.as_char(), 'P');
    /// ```
    pub const fn as_char(self) -> char {
        match self {
            SystemEventCode::StartOfMessages => 'O',
            SystemEventCode::EndOfMessages => 'C',
            SystemEventCode::StartOfMarketHours => 'Q',
            SystemEventCode::EndOfMarketHours => 'M',
            SystemEventCode::StartOfSystemHours => 'S',
            SystemEventCode::EndOfSystemHours => 'E',
            SystemEventCode::MarketPreopen => 'P',
        }
    }

    /// Interpret the ITCH system-event char.
    ///
    /// Returns `None` for any char outside the code set
    /// (`O`/`C`/`Q`/`M`/`S`/`E`/`P`).
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_contracts::SystemEventCode;
    /// assert_eq!(SystemEventCode::from_char('O'),
    ///     Some(SystemEventCode::StartOfMessages));
    /// assert_eq!(SystemEventCode::from_char('Z'), None);
    /// ```
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'O' => Some(SystemEventCode::StartOfMessages),
            'C' => Some(SystemEventCode::EndOfMessages),
            'Q' => Some(SystemEventCode::StartOfMarketHours),
            'M' => Some(SystemEventCode::EndOfMarketHours),
            'S' => Some(SystemEventCode::StartOfSystemHours),
            'E' => Some(SystemEventCode::EndOfSystemHours),
            'P' => Some(SystemEventCode::MarketPreopen),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip every enum against the wire chars — anchored to
    /// `behavior-catalog/messages.json` §me_enums.
    #[test]
    fn wire_chars_match_catalog() {
        for (c, expect) in [
            ('L', OrderType::Limit),
            ('F', OrderType::Fok),
            ('I', OrderType::Ioc),
            ('P', OrderType::PostOnly),
            ('B', OrderType::Bypass),
        ] {
            assert_eq!(OrderType::from_char(c), Some(expect));
            assert_eq!(expect.as_char(), c);
        }
        for (c, expect) in [('B', Side::Bid), ('A', Side::Ask)] {
            assert_eq!(Side::from_char(c), Some(expect));
            assert_eq!(expect.as_char(), c);
        }
        for (c, expect) in [('D', Visibility::Display), ('H', Visibility::Hidden)] {
            assert_eq!(Visibility::from_char(c), Some(expect));
            assert_eq!(expect.as_char(), c);
        }
    }

    #[test]
    fn kill_reasons_match_catalog() {
        for (c, expect) in [
            ('H', KillReason::InstrumentHalted),
            ('C', KillReason::ClientRequest),
            ('A', KillReason::AdminRequest),
            ('I', KillReason::Ioc),
            ('F', KillReason::Fok),
            ('S', KillReason::SelfTrade),
            ('R', KillReason::CrossMarket),
            ('P', KillReason::PriceProtection),
            ('M', KillReason::ModifyRequest),
            ('E', KillReason::ExternalReject),
        ] {
            assert_eq!(KillReason::from_char(c), Some(expect), "char {c}");
            assert_eq!(expect.as_char(), c);
        }
    }

    #[test]
    fn system_events_match_catalog() {
        for (c, expect) in [
            ('O', SystemEventCode::StartOfMessages),
            ('C', SystemEventCode::EndOfMessages),
            ('Q', SystemEventCode::StartOfMarketHours),
            ('M', SystemEventCode::EndOfMarketHours),
            ('S', SystemEventCode::StartOfSystemHours),
            ('E', SystemEventCode::EndOfSystemHours),
            ('P', SystemEventCode::MarketPreopen),
        ] {
            assert_eq!(SystemEventCode::from_char(c), Some(expect), "char {c}");
            assert_eq!(expect.as_char(), c);
        }
    }

    #[test]
    fn side_opposite() {
        assert_eq!(Side::Bid.opposite(), Side::Ask);
        assert_eq!(Side::Ask.opposite(), Side::Bid);
    }
}
