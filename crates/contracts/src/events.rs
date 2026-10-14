// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 domain events — Sentin8:Bus envelopes plus the matching
//! engine's inbound/outbound message payloads.
//!
//! Event names follow `exchange8:<service>:<resource>:<operation>`
//! (colons only). The same `Envelope<T>` shape encodes both bus produce
//! payloads and the framed direct streams between ME and its clients; the
//! inbound DTOs mirror the ME wire messages one-for-one (catalog
//! `me_messages.inbound`), the outbound DTOs the lifecycle broadcasts
//! (`me_messages.outbound`).
//!
//! Design: `crates/contracts/DESIGN.md` §3 (data model),
//! `design/sentin8-exchange8-technical-design.md` §7 (event architecture).

use exchange8_fixed::{Price, Qty};
use serde::{Deserialize, Serialize};

use crate::enums::{
    Capacity, InstrumentStatus, KillReason, Liquidity, LotType, MarketStatus, MeRejectReason, Side,
};

/// Event envelope with idempotency key (Sentin8:Bus inbox-first pattern).
///
/// Wraps a domain payload with the identity fields any consumer needs to
/// route (tenant), order (sequence), and dedup (idempotency key) the event,
/// regardless of transport (bus produce or framed direct stream).
///
/// # Examples
///
 /// ```
/// use exchange8_contracts::{Envelope, OrderKilled, KillReason};
///
/// let env: Envelope<OrderKilled> = Envelope {
///     event_type: "exchange8:me:order:killed".into(),
///     env_id: "bank-ops".into(),
///     seq: 3,
///     idempotency_key: "me-7-42".into(),
///     payload: OrderKilled {
///         order_id: 42,
///         sub_order_id: 1,
///         instrument_id: 7,
///         connection_id: 3,
///         reason: KillReason::Ioc,
///     },
/// };
/// assert_eq!(env.seq, 3);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    /// `exchange8:<service>:<resource>:<operation>` (colons only).
    pub event_type: String,
    /// Tenant environment.
    pub env_id: String,
    /// Monotonic per-emitter sequence.
    pub seq: u64,
    /// Dedup key for exactly-once consumers.
    pub idempotency_key: String,
    /// Domain payload.
    pub payload: T,
}

// ---------------------------------------------------------------------------
// ME order-entry messages (in) — the wire spec inbound
// ---------------------------------------------------------------------------

/// `ME_MSG_ADD_ORDER` ('a') — `MEAddOrderMsg`.
///
/// The order-entry operation; every matching-engine order begins here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddOrder {
    /// Limit price (scaled).
    pub price: Price,
    /// Engine order id.
    pub order_id: u64,
    /// Sub-order id (iceberg slices, peg re-prices).
    pub sub_order_id: u32,
    /// Instrument the order trades.
    pub instrument_id: u32,
    /// Ordered quantity (scaled).
    pub qty: Qty,
    /// Gateway session that submitted the order (risk routing key).
    pub connection_id: u16,
    /// Book side (`B`/`A`).
    pub side: Side,
    /// ME order type (`L`/`F`/`I`/`P`/`B`).
    pub order_type: crate::enums::OrderType,
    /// Displayed vs hidden quantity.
    pub visibility: crate::enums::Visibility,
    /// Agency vs principal capacity.
    pub capacity: Capacity,
    /// Client account (FIX 1).
    pub account: String,
    /// STP key value; `None` disables STP (mpid=0).
    pub stp_key: Option<u64>,
    /// STP action when a self-trade would occur.
    pub stp_action: Option<crate::order::StpAction>,
    /// Customer id for reporting (0 = anonymous).
    pub customer_id: u16,
    /// Book the order routes to.
    pub book_id: u16,
    /// Publish order details anonymously on feeds.
    pub anonymous: bool,
    /// Price-protection behavior on a protection-band violation.
    pub price_protection: Option<crate::enums::PriceProtection>,
    /// Iceberg display floor.
    pub max_floor: Qty,
    /// Strategy book tokens; `*` = all books.
    pub strategy: Vec<u8>,
}

/// `ME_MSG_CANCEL_ORDER` ('c') — `MECancelOrderMsg`.
///
/// Cancels the identified resting order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelOrder {
    /// Engine order id to cancel.
    pub order_id: u64,
    /// Instrument the order rests on.
    pub instrument_id: u32,
    /// Owning gateway session (must match the order's).
    pub connection_id: u16,
}

/// `ME_MSG_CANCEL_ALL_CONNECTION_ORDERS` ('d').
///
/// Kills every resting order of one gateway session (session drop,
/// disconnect cleanup).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CancelAllForConnection {
    /// Session whose resting orders are killed.
    pub connection_id: u16,
}

/// `ME_MSG_MODIFY_ORDER` ('u') — `MEModifyOrderMsg`.
///
/// Replace of a resting order: a new order id is allocated and the
/// quantity delta may be positive (add) or negative (reduce).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifyOrder {
    /// Replacement limit price (scaled).
    pub new_price: Price,
    /// Engine order id being replaced.
    pub order_id: u64,
    /// Instrument the order rests on.
    pub instrument_id: u32,
    /// Engine order id allocated to the replacement.
    pub new_order_id: u64,
    /// Signed quantity change (scaled; negative reduces).
    pub delta_qty: i64,
    /// Owning gateway session.
    pub connection_id: u16,
    /// New iceberg display floor.
    pub new_max_floor: Qty,
}

/// `ME_MSG_INSTRUMENT_STATUS` ('S') — `MEInstrumentStatusMsg`.
///
/// Per-instrument trading-state change (open/halted/quotation-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentStatusMsg {
    /// Instrument whose status changed.
    pub instrument_id: u32,
    /// New instrument status.
    pub status: InstrumentStatus,
}

/// `ME_MSG_MARKET_STATUS` ('M') — `MEMarketStatusMsg`.
///
/// Per-book market-state change (reopen/halt/postpone/shutdown).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketStatusMsg {
    /// Book whose status changed.
    pub book_id: u16,
    /// New market status.
    pub status: MarketStatus,
}

/// `ME_MSG_ONE_ORDER_STATE_REQUEST` ('V') / `ME_MSG_ORDER_STATE` ('U') /
/// `ME_MSG_ERASE_ORDER` ('X') — `MEOrderStateMsg`.
///
/// One message shape serving three tags: the state inquiry ('V'), the
/// state response ('U') and the book erase ('X').
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderStateMsg {
    /// Order being inquired, responded, or erased.
    pub order_id: u64,
    /// Instrument the order rests on.
    pub instrument_id: u32,
    /// Volume associated with the request/response.
    pub volume: Qty,
    /// Owning gateway session.
    pub connection_id: u16,
    /// `'Y'` → also notify killed when not found.
    pub notify_killed: bool,
}

// ---------------------------------------------------------------------------
// ME lifecycle messages (out) — the wire spec outbound
// ---------------------------------------------------------------------------

/// `ME_MSG_ORDER_BOOKED` ('B') — `MEOrderBookedMsg`.
///
/// The order now rests on the book (status `New` / `Replaced`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderBooked {
    /// Resting price (scaled); 0 if the order was not repriced.
    pub price: Price,
    /// Engine order id.
    pub order_id: u64,
    /// Sub-order id (iceberg slices, peg re-prices).
    pub sub_order_id: u32,
    /// Instrument the order rests on.
    pub instrument_id: u32,
    /// Visible resting volume (scaled).
    pub volume: Qty,
    /// Owning gateway session.
    pub connection_id: u16,
}

/// `ME_MSG_ORDER_KILLED` ('K') — `MEOrderKilledMsg`.
///
/// The order left the book; `reason` says why (catalog `kill_reason`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderKilled {
    /// Engine order id killed.
    pub order_id: u64,
    /// Sub-order id killed.
    pub sub_order_id: u32,
    /// Instrument the order rested on.
    pub instrument_id: u32,
    /// Owning gateway session.
    pub connection_id: u16,
    /// Why the order was killed.
    pub reason: KillReason,
}

/// `ME_MSG_ORDER_MODIFIED` ('C') — `MEOrderModifiedMsg`.
///
/// The replace completed; the new order id carries the replaced order's
/// time priority forward.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderModified {
    /// Replacement limit price (scaled).
    pub new_price: Price,
    /// Replaced engine order id.
    pub order_id: u64,
    /// Replaced sub-order id.
    pub sub_order_id: u32,
    /// Instrument the order rests on.
    pub instrument_id: u32,
    /// New visible resting volume (scaled).
    pub new_volume: Qty,
    /// Replacement engine order id.
    pub new_order_id: u64,
    /// Owning gateway session.
    pub connection_id: u16,
    /// Book side of the order.
    pub side: Side,
    /// New iceberg display floor.
    pub new_max_floor: Qty,
}

/// `ME_MSG_ORDER_FILLED` ('F') — `MEOrderFilledMsg`.
///
/// The order is completely filled (terminal status `Fill`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderFilled {
    /// Engine order id completely filled.
    pub order_id: u64,
    /// Instrument the order traded.
    pub instrument_id: u32,
    /// Owning gateway session.
    pub connection_id: u16,
}

/// `ME_MSG_ORDER_ACCEPTED` ('A') — `MEOrderAcceptedMsg`.
///
/// Acknowledgment that the order passed validation (status `New` pending
/// the booking broadcast).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderAccepted {
    /// Engine order id accepted.
    pub order_id: u64,
    /// Instrument the order targets.
    pub instrument_id: u32,
    /// Owning gateway session.
    pub connection_id: u16,
}

/// `ME_MSG_ORDER_TRANSACTION_REJECTED` ('R') —
/// `MEOrderTransactionRejectedMsg`.
///
/// The named operation (`Add`/`Cancel`/`Modify`) failed; `reason` says why.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransactionRejected {
    /// Engine order id the failed operation targeted.
    pub order_id: u64,
    /// Instrument the operation targeted.
    pub instrument_id: u32,
    /// Owning gateway session.
    pub connection_id: u16,
    /// Why the operation was rejected (catalog `reject_reason`).
    pub reason: MeRejectReason,
    /// Which operation was rejected.
    pub rejected_op: RejectedOp,
}

/// `rejectedMsgType` values — the operation a `TransactionRejected` refers
/// to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectedOp {
    /// The add-order operation was rejected (no order was created).
    Add,
    /// The cancel operation was rejected (the order still rests).
    Cancel,
    /// The modify/replace operation was rejected.
    Modify,
}

/// `ME_MSG_ORDER_EXECUTED` ('E') — `MEOrderExecutedMsg`.
///
/// **One per side per execution** with a common `trade_id`; the active side
/// is `Liquidity::Removed`, the passive `Liquidity::Added` (the book
/// listener's `Trade` update).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderExecuted {
    /// Trade id shared by both sides of the execution.
    pub trade_id: u64,
    /// Execution timestamp, microseconds since the Unix epoch.
    pub ts_us: u64,
    /// Execution price (scaled).
    pub price: Price,
    /// This side's engine order id.
    pub order_id: u64,
    /// This side's sub-order id.
    pub sub_order_id: u32,
    /// Counterparty engine order id.
    pub cp_order_id: u64,
    /// Counterparty sub-order id.
    pub cp_sub_order_id: u32,
    /// Instrument traded.
    pub instrument_id: u32,
    /// Executed quantity (scaled).
    pub qty: Qty,
    /// Open quantity on this order after the execution (scaled).
    pub remaining_volume: Qty,
    /// Owning gateway session.
    pub connection_id: u16,
    /// Passive customer id when the counterparty is not anonymous.
    pub counterparty_id: Option<u16>,
    /// Book the execution printed on.
    pub book_id: u16,
    /// This side's liquidity role (added passive / removed aggressor).
    pub liquidity: Liquidity,
    /// Book side of this event's order.
    pub side: Side,
    /// Lot classification of the execution (odd vs board).
    pub lot_type: LotType,
    /// Last market MIC (4 bytes, space-padded).
    pub last_mkt: [u8; 4],
    /// Executed as principal (firm's own account).
    pub principal_trade: bool,
    /// Flagged as a wash trade (regulatory review marker).
    pub wash_trade: bool,
    /// `M` = midpoint (peg/iceberg sub-order), else `R` regular.
    pub midpoint_charge: bool,
}

/// `ME_MSG_ORDER_REPRICED` ('P') — `MEOrderRepricedMsg`.
///
/// A resting order's price changed without a client request — currently the
/// price-protection sidestep.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderRepriced {
    /// New resting price (scaled).
    pub price: Price,
    /// Engine order id repriced.
    pub order_id: u64,
    /// Sub-order id repriced.
    pub sub_order_id: u32,
    /// Instrument the order rests on.
    pub instrument_id: u32,
    /// Owning gateway session.
    pub connection_id: u16,
    /// Why the price changed (price protection sidestep).
    pub reason: KillReason,
}

// ---------------------------------------------------------------------------
// Quote events (VFeeds/MD → OMS/ME/SOR)
// ---------------------------------------------------------------------------

/// Best-bid/offer snapshot (own-book BBO or consolidated EBBO).
///
/// Prices are `None` when that side of the book is empty.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Quote {
    /// Best bid price; `None` = no bid (previously encoded as 0).
    pub bid: Option<Price>,
    /// Best ask price; `None` = no ask (previously encoded as 0).
    pub ask: Option<Price>,
    /// Snapshot timestamp, microseconds since the Unix epoch.
    pub ts_us: u64,
}

/// Consolidated best-bid/offer update — the
/// `exchange8:md:bbo:updated` / `exchange8:vfeeds:index:updated` payloads.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BboUpdated {
    /// Instrument the quote belongs to.
    pub instrument_id: u32,
    /// The consolidated quote snapshot.
    pub quote: Quote,
    /// Source MICs (EBBO) for the bid side.
    pub bid_mic: Option<[u8; 4]>,
    /// Source MICs (EBBO) for the ask side.
    pub ask_mic: Option<[u8; 4]>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Envelope JSON round-trip keeps the event name and payload intact.
    #[test]
    fn envelope_round_trip() {
        let env = Envelope {
            event_type: "exchange8:me:trade:executed".into(),
            env_id: "test-env".into(),
            seq: 1,
            idempotency_key: "k1".into(),
            payload: OrderKilled {
                order_id: 42,
                sub_order_id: 1,
                instrument_id: 7,
                connection_id: 3,
                reason: KillReason::Ioc,
            },
        };
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("exchange8:me:trade:executed"));
        let back: Envelope<OrderKilled> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.payload.order_id, 42);
    }
}
