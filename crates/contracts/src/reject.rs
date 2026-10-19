// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 reject-code catalog with FIX 103 text - 1:1 port of the
//! protocol's rejection numbers.
//!
//! Truth: `design/behavior-catalog/reject-codes.json` (the `order_reject`,
//! `base_reject`, and `cancel_reject` families). Codes the platform adds on
//! top (risk unavailability, rate limiting, Greeks limits, margin,
//! malformed, internal) deliberately carry no numeric wire code - they are
//! internal failure vocabulary, never presented as legacy numeric codes.
//!
//! Design: `crates/contracts/DESIGN.md` §4 (API surface), §5 (error-code
//! vocabulary), §6 (failure vocabulary).

use serde::{Deserialize, Serialize};

/// Standardized reject reason carried in exec reports / cancel rejects.
///
/// Spans the order-reject, base-reject, and cancel-reject families of the
/// extracted catalog plus the platform-added reasons that have no numeric
/// wire code. `#[non_exhaustive]`: downstream matches must handle future
/// variants.
///
/// # Examples
/// ```
/// use exchange8_contracts::RejectCode;
///
/// let code = RejectCode::WrongPrice;
/// assert_eq!(code.wire_code(), Some(120));
/// assert_eq!(code.as_str(), "Wrong price");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RejectCode {
    // -- order_reject family (numeric codes 1-8, 100-144) -------------------
    /// `1` - unknown symbol (instrument not provisioned).
    UnknownSymbol,
    /// `2` - the exchange is closed.
    ExchangeClosed,
    /// `3` - the order exceeds the capitalization limit.
    OrderExceedsLimit,
    /// `4` - outside the order-entry window.
    TooLateToEnter,
    /// `5` - unknown order (id not on the book).
    UnknownOrder,
    /// `6` - duplicate order reference.
    DuplicateOrder,
    /// `7` - duplicate verbal order reference.
    DuplicateVerbalOrder,
    /// `8` - stale order (sequence number too old).
    StaleOrder,
    /// `100` - unknown security exchange (MIC unrecognized).
    InvalidSecurityExchange,
    /// `101` - the stock is not shortable.
    StockNotShortable,
    /// `102` - invalid security id (instrument snapshot mismatch).
    InvalidSecurityId,
    /// `103` - invalid symbol (unparseable or empty).
    InvalidSymbol,
    /// `104` - wrong order type for the session/instrument.
    WrongOrderType,
    /// `105` - the client is suspended.
    ClientSuspended,
    /// `106` - wrong account for this session.
    WrongAccount,
    /// `107` - wrong broker/company id.
    WrongBrokerId,
    /// `108` - the exchange is halted.
    ExchangeHalted,
    /// `109` - the security is halted on the ATS book.
    SecurityHaltedAts,
    /// `110` - the security is halted on the primary book.
    SecurityHaltedPrimary,
    /// `111` - the security is under holiday trading rules.
    SecurityHoliday,
    /// `112` - the security's trading session is closed.
    SecurityTradingClosed,
    /// `113` - the exchange postponed the session.
    ExchangePostponed,
    /// `114` - the exchange is in the pre-open period.
    ExchangePreopen,
    /// `115` - invalid routing destination.
    InvalidRouting,
    /// `116` - wrong visibility flag (hidden/display inconsistency).
    WrongVisibleFlag,
    /// `117` - order not large-in-scale (LIS threshold not met).
    NotLargeInScale,
    /// `118` - wrong order capacity (agency/principal inconsistency).
    WrongOrderCapacity,
    /// `119` - wrong currency for the instrument.
    WrongCurrency,
    /// `120` - wrong price (off-tick or non-positive).
    WrongPrice,
    /// `121` - wrong quantity (lot-size or scaling violation).
    WrongQuantity,
    /// `122` - consolidated (EBBO) pegging not allowed on this book.
    EbboNotAllowed,
    /// `123` - wrong post-only marker (side or session rules).
    WrongPostOnly,
    /// `124` - wrong user id for the session.
    WrongUserId,
    /// `130` - wrong time in force for the session/instrument.
    WrongTimeInForce,
    /// `131` - wrong bypass marker (hidden-level matching misuse).
    WrongBypassMarker,
    /// `132` - price does not match the instrument's tick size.
    MismatchPriceTicksize,
    /// `133` - wrong price-protection marker.
    WrongPriceProtection,
    /// `134` - wrong cross type for the cross order.
    WrongCrossType,
    /// `135` - session lacks the required entitlement.
    InvalidEntitlement,
    /// `136` - wrong obligor account type id.
    WrongAccountTypeId,
    /// `137` - invalid jitney (carry broker) reference.
    InvalidJitney,
    /// `138` - wrong short-marking-exempt marker.
    WrongShortMarkingExempt,
    /// `140` - wrong iceberg max-floor value.
    WrongMaxFloor,
    /// `141` - wrong peg difference (scale or sign).
    WrongPegDifference,
    /// `142` - wrong order book (book token not enabled).
    WrongOrderBook,
    /// `143` - wrong order side for the operation.
    WrongOrderSide,
    /// `144` - wrong execution-instruction combination.
    WrongExecInst,
    // -- base_reject family (numeric codes 125-129, 200) ---------------------
    /// `125` - odd lot not accepted on this book.
    OddLot,
    /// `126` - mixed odd/board lot composition not allowed.
    MixedLot,
    /// `127` - short sale restricted by the uptick rule.
    UptickRule,
    /// `128` - below the half-penny tick restriction.
    HalfPennyTick,
    /// `129` - below the full-penny tick restriction.
    FullPennyTick,
    /// `200` - unspecified/legacy reason without a specific variant.
    UnknownReason,
    // -- cancel_reject family (numeric codes 0, 2, 3, 105) -------------------
    /// cancel `0` - too late to cancel (already executing or done).
    TooLate,
    /// cancel `2` - rejected at the broker's discretion.
    BrokerOption,
    /// cancel `3` - a cancel/replace is already pending on the order.
    AlreadyPending,
    /// cancel `105` - replacement quantity invalid.
    WrongReplaceQty,
    // -- platform-added (no numeric wire code) -------------------------------
    /// The risk authorization service is unavailable - fail-closed by
    /// design (Risk service design §9).
    RiskUnavailable,
    /// The per-session rate guard rejected the request (Risk design §5.3).
    RateLimited,
    /// A Greeks-based options limit was exceeded (options extension §3.2).
    GreeksLimit,
    /// The order requires more margin than the account can post.
    MarginInsufficient,
    /// The message failed structural validation before semantics.
    Malformed,
    /// An internal fault with no more specific classification
    /// (fail-closed).
    Internal,
}

impl RejectCode {
    /// Numeric code (the wire spec) where one exists.
    ///
    /// The value is used in ExecType=8 rejects (FIX `OrdRejReason`) for
    /// client compatibility; codes without a spec mapping return `None` and
    /// must not be presented as numeric codes to clients.
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::RejectCode;
    /// assert_eq!(RejectCode::UnknownSymbol.wire_code(), Some(1));
    /// assert_eq!(RejectCode::RateLimited.wire_code(), None);
    /// ```
    pub fn wire_code(self) -> Option<u16> {
        Some(match self {
            RejectCode::UnknownSymbol => 1,
            RejectCode::ExchangeClosed => 2,
            RejectCode::OrderExceedsLimit => 3,
            RejectCode::TooLateToEnter => 4,
            RejectCode::UnknownOrder => 5,
            RejectCode::DuplicateOrder => 6,
            RejectCode::DuplicateVerbalOrder => 7,
            RejectCode::StaleOrder => 8,
            RejectCode::InvalidSecurityExchange => 100,
            RejectCode::StockNotShortable => 101,
            RejectCode::InvalidSecurityId => 102,
            RejectCode::InvalidSymbol => 103,
            RejectCode::WrongOrderType => 104,
            RejectCode::ClientSuspended => 105,
            RejectCode::WrongAccount => 106,
            RejectCode::WrongBrokerId => 107,
            RejectCode::ExchangeHalted => 108,
            RejectCode::SecurityHaltedAts => 109,
            RejectCode::SecurityHaltedPrimary => 110,
            RejectCode::SecurityHoliday => 111,
            RejectCode::SecurityTradingClosed => 112,
            RejectCode::ExchangePostponed => 113,
            RejectCode::ExchangePreopen => 114,
            RejectCode::InvalidRouting => 115,
            RejectCode::WrongVisibleFlag => 116,
            RejectCode::NotLargeInScale => 117,
            RejectCode::WrongOrderCapacity => 118,
            RejectCode::WrongCurrency => 119,
            RejectCode::WrongPrice => 120,
            RejectCode::WrongQuantity => 121,
            RejectCode::EbboNotAllowed => 122,
            RejectCode::WrongPostOnly => 123,
            RejectCode::WrongUserId => 124,
            RejectCode::WrongTimeInForce => 130,
            RejectCode::WrongBypassMarker => 131,
            RejectCode::MismatchPriceTicksize => 132,
            RejectCode::WrongPriceProtection => 133,
            RejectCode::WrongCrossType => 134,
            RejectCode::InvalidEntitlement => 135,
            RejectCode::WrongAccountTypeId => 136,
            RejectCode::InvalidJitney => 137,
            RejectCode::WrongShortMarkingExempt => 138,
            RejectCode::WrongMaxFloor => 140,
            RejectCode::WrongPegDifference => 141,
            RejectCode::WrongOrderBook => 142,
            RejectCode::WrongOrderSide => 143,
            RejectCode::WrongExecInst => 144,
            RejectCode::OddLot => 125,
            RejectCode::MixedLot => 126,
            RejectCode::UptickRule => 127,
            RejectCode::HalfPennyTick => 128,
            RejectCode::FullPennyTick => 129,
            RejectCode::UnknownReason => 200,
            RejectCode::TooLate => 0,
            RejectCode::BrokerOption => 2,
            RejectCode::AlreadyPending => 3,
            RejectCode::WrongReplaceQty => 105,
            _ => return None,
        })
    }

    /// FIX 103 (OrdRejReason) human-readable text - matches the
    /// catalog's rejection text strings.
    ///
    /// The returned text is static per variant; clients may rely on it
    /// unchanged.
    ///
    /// # Examples
    /// ```
    /// use exchange8_contracts::RejectCode;
    /// assert_eq!(RejectCode::UnknownSymbol.as_str(), "Unknown symbol");
    /// assert_eq!(RejectCode::RateLimited.as_str(), "Rate limited");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            RejectCode::UnknownSymbol => "Unknown symbol",
            RejectCode::ExchangeClosed => "Exchange closed",
            RejectCode::OrderExceedsLimit => "Order exceeds capitalization limit",
            RejectCode::TooLateToEnter => "Too late to enter",
            RejectCode::UnknownOrder => "Unknown order",
            RejectCode::DuplicateOrder => "Duplicate order",
            RejectCode::DuplicateVerbalOrder => "Duplicate verbal order",
            RejectCode::StaleOrder => "Stale order",
            RejectCode::InvalidSecurityExchange => "Invalid security exchange",
            RejectCode::StockNotShortable => "Stock not shortable",
            RejectCode::InvalidSecurityId => "Invalid security id",
            RejectCode::InvalidSymbol => "Invalid symbol",
            RejectCode::WrongOrderType => "Wrong order type",
            RejectCode::ClientSuspended => "Client suspended",
            RejectCode::WrongAccount => "Wrong account",
            RejectCode::WrongBrokerId => "Wrong broker id",
            RejectCode::ExchangeHalted => "Exchange halted",
            RejectCode::SecurityHaltedAts => "Security halted at ATS",
            RejectCode::SecurityHaltedPrimary => "Security halted at primary",
            RejectCode::SecurityHoliday => "Security holiday",
            RejectCode::SecurityTradingClosed => "Security trading closed",
            RejectCode::ExchangePostponed => "Exchange postponed",
            RejectCode::ExchangePreopen => "Exchange preopen",
            RejectCode::InvalidRouting => "Invalid routing",
            RejectCode::WrongVisibleFlag => "Wrong visible flag",
            RejectCode::NotLargeInScale => "Not large in scale",
            RejectCode::WrongOrderCapacity => "Wrong order capacity",
            RejectCode::WrongCurrency => "Wrong currency",
            RejectCode::WrongPrice => "Wrong price",
            RejectCode::WrongQuantity => "Wrong quantity",
            RejectCode::EbboNotAllowed => "EBBO not allowed",
            RejectCode::WrongPostOnly => "Wrong post only",
            RejectCode::WrongUserId => "Wrong user id",
            RejectCode::WrongTimeInForce => "Wrong time in force",
            RejectCode::WrongBypassMarker => "Wrong bypass marker",
            RejectCode::MismatchPriceTicksize => "Price not on tick grid",
            RejectCode::WrongPriceProtection => "Wrong price protection",
            RejectCode::WrongCrossType => "Wrong cross type",
            RejectCode::InvalidEntitlement => "Invalid entitlement",
            RejectCode::WrongAccountTypeId => "Wrong account type id",
            RejectCode::InvalidJitney => "Invalid jitney",
            RejectCode::WrongShortMarkingExempt => "Wrong short marking exempt",
            RejectCode::WrongMaxFloor => "Wrong max floor",
            RejectCode::WrongPegDifference => "Wrong peg difference",
            RejectCode::WrongOrderBook => "Wrong order book",
            RejectCode::WrongOrderSide => "Wrong order side",
            RejectCode::WrongExecInst => "Wrong exec inst",
            RejectCode::OddLot => "Odd lot",
            RejectCode::MixedLot => "Mixed lot",
            RejectCode::UptickRule => "Uptick rule",
            RejectCode::HalfPennyTick => "Half penny tick",
            RejectCode::FullPennyTick => "Full penny tick",
            RejectCode::UnknownReason => "Unknown reason",
            RejectCode::TooLate => "Too late to cancel",
            RejectCode::BrokerOption => "Broker option",
            RejectCode::AlreadyPending => "Already pending",
            RejectCode::WrongReplaceQty => "Wrong replace quantity",
            RejectCode::RiskUnavailable => "Risk service unavailable",
            RejectCode::RateLimited => "Rate limited",
            RejectCode::GreeksLimit => "Greeks limit exceeded",
            RejectCode::MarginInsufficient => "Insufficient margin",
            RejectCode::Malformed => "Malformed message",
            RejectCode::Internal => "Internal error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anchored to `behavior-catalog/reject-codes.json`: every code has
    /// a matching enum variant with the exact numeric value.
    /// Every catalog reject has a matching variant with the exact numeric
    /// value; platform rejects have none.
    #[test]
    fn wire_codes_match_catalog() {
        // order_reject samples (catalog order_reject[])
        assert_eq!(RejectCode::UnknownSymbol.wire_code(), Some(1));
        assert_eq!(RejectCode::ExchangeClosed.wire_code(), Some(2));
        assert_eq!(RejectCode::OrderExceedsLimit.wire_code(), Some(3));
        assert_eq!(RejectCode::InvalidSecurityExchange.wire_code(), Some(100));
        assert_eq!(RejectCode::WrongExecInst.wire_code(), Some(144));
        // base_reject
        assert_eq!(RejectCode::OddLot.wire_code(), Some(125));
        assert_eq!(RejectCode::UnknownReason.wire_code(), Some(200));
        // cancel_reject
        assert_eq!(RejectCode::TooLate.wire_code(), Some(0));
        assert_eq!(RejectCode::AlreadyPending.wire_code(), Some(3));
        // new codes have no mapping
        assert_eq!(RejectCode::RiskUnavailable.wire_code(), None);
        assert_eq!(RejectCode::RateLimited.wire_code(), None);
    }

    /// Every sampled variant carries non-empty FIX 103 text.
    #[test]
    fn fix103_text_present() {
        for code in [
            RejectCode::UnknownSymbol,
            RejectCode::MismatchPriceTicksize,
            RejectCode::NotLargeInScale,
            RejectCode::TooLate,
            RejectCode::RiskUnavailable,
        ] {
            assert!(!code.as_str().is_empty(), "{code:?}");
        }
    }
}
