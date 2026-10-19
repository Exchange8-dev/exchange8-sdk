// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 contracts - the shared domain vocabulary of the exchange8
//! vertical.
//!
//! Every service in the orders/rejects flow (gateway → risk → me →
//! clearing) imports its order, reject, event, and risk-contract types from
//! this crate instead of declaring its own variants, so the wire posture is
//! defined exactly once.
//!
//! # Quick start
//!
//! Building an envelope with a lifecycle event as it flows from the
//! matching engine to its consumers:
//!
//! ```
//! use exchange8_contracts::{Envelope, KillReason, OrderKilled};
//!
//! let env = Envelope {
//!     event_type: "exchange8:me:order:killed".to_string(),
//!     env_id: "ops".to_string(),
//!     seq: 1,
//!     idempotency_key: "me-4-17".to_string(),
//!     payload: OrderKilled {
//!         order_id: 17,
//!         sub_order_id: 0,
//!         instrument_id: 4,
//!         connection_id: 2,
//!         reason: KillReason::Ioc,
//!     },
//! };
//! let json = serde_json::to_string(&env).unwrap();
//! assert!(json.contains("exchange8:me:order:killed"));
//! ```
//!
//! Mapping a FIX client-facing rejection:
//!
//! ```
//! use exchange8_contracts::RejectCode;
//!
//! let code = RejectCode::WrongPrice;
//! assert_eq!(code.wire_code(), Some(120)); // FIX OrdRejReason value
//! assert_eq!(code.as_str(), "Wrong price"); // FIX 103 text
//! ```
//!
//! # Module map
//!
//! - [`order`]: normalized order/instrument DTOs (FIX 4.2 superset)
//! - [`reject`]: reject-code catalog with FIX 103 text
//! - [`events`]: bus envelopes + matching-engine event payloads
//! - [`enums`]: shared char-code enums (order types, sides, kill/reject reasons)
//! - [`risk`]: Gateway → Risk authorize wire DTOs (cross-service API surface)
//!
//! Re-exports are explicit (no globs) - facade-level names stay collision-free.
//!
//! # Design documents
//!
//! Overview: `crates/contracts/README.md`. Detailed design:
//! `crates/contracts/DESIGN.md` (data model §3, API surface §4, numeric and
//! wire-format conventions §5). Business view: `crates/contracts/BUSINESS.md`.
//!
//! Behavior truth: `design/behavior-catalog/messages.json` +
//! `reject-codes.json` (the extracted wire constants this crate encodes
//! 1:1).

pub mod enums;
pub mod events;
pub mod order;
pub mod reject;
pub mod risk;

pub use enums::{
    Capacity, InstrumentStatus, KillReason, Liquidity, LotType, MarketStatus, MeRejectReason,
    OrderType, PriceProtection, Side, SystemEventCode, Visibility,
};
pub use events::{
    AddOrder, BboUpdated, CancelAllForConnection, CancelOrder, Envelope, InstrumentStatusMsg,
    MarketStatusMsg, ModifyOrder, OrderAccepted, OrderBooked, OrderExecuted, OrderFilled,
    OrderKilled, OrderModified, OrderRepriced, OrderStateMsg, Quote, RejectedOp,
    TransactionRejected,
};
pub use order::{
    BookToken, CrossData, ExchangeOrder, InstrumentKind, InstrumentRef, OrdStatus, OrdType,
    OrderSide, PegType, StpAction, StpKeyType, Tif,
};
pub use reject::RejectCode;
pub use risk::{AuthorizeRequest, AuthorizeResponse, Decision};
