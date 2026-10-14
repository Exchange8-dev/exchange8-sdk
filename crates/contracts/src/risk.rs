// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Risk authorize wire DTOs — the Gateway → Risk contract
//! (Risk design §7, master design §4.1 namespace rule).
//!
//! Cross-service API surface lives in the shared contracts crate, not in a
//! single-service client crate. The facade re-exports this module as
//! `exchange8::contracts::risk` (explicit, no glob — keeps facade-level
//! names collision-free).
//!
//! Design: `crates/contracts/DESIGN.md` §4 (API surface — risk contract).

use serde::{Deserialize, Serialize};

/// `POST /v1/authorize` request — the normalized order.
///
/// The gateway decodes the client session into an [`ExchangeOrder`] and
/// submits it unchanged; Risk's decision applies to exactly this
/// representation (no side-channel fields).
pub type AuthorizeRequest = crate::order::ExchangeOrder;

/// `POST /v1/authorize` response — the pre-trade decision.
///
/// `code` is `Some` exactly when `decision` is [`Decision::Reject`]; the
/// code comes from the shared reject vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizeResponse {
    /// The decision (allow or reject).
    pub decision: Decision,
    /// Reject code when denied.
    pub code: Option<crate::reject::RejectCode>,
}

/// Wire decision (allow/reject — the wire protocol's two outcomes).
///
/// Fail-closed vocabulary (`RiskUnavailable`, `RateLimited`, …) is carried
/// on [`AuthorizeResponse::code`], not as extra decision variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// The order passed the pre-trade checks.
    Allow,
    /// The order was denied; see the response's reject code.
    Reject,
}
