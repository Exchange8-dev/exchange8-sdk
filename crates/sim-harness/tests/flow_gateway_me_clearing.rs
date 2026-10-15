// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 cross-service flow test — the sim-harness pattern.
//!
//! Drives the real service-core crates across the submit→match→project
//! chain — layers unit tests only ever see in isolation:
//!
//!   Gateway submission policy (`submission.rs::route_order` accept +
//!   reject) → ME matching (`me::engine::MatchingEngine`) → `MeEvent`
//!   stream → Clearing projectors (`OrderProjector` + `TradeProjector`).
//!
//! DB-free by construction: projectors produce in-memory rows; persistence
//! is covered by the per-service `sql_matrix.rs` suites, the route/ME/
//! projection invariants by the in-module unit tests.
//!
//! Design: `crates/sim-harness/DESIGN.md` §2 (core path), §9 (tree-layout
//! embedding contract),
//! `design/sentin8-exchange8-technical-design.md` §13b (verification &
//! parity).

// Tree-layout-only target: with the feature off (single-repo CI) the
// whole file contributes zero tests; with `--features tree_flows`
// (aggregation-tree layout) the REAL service cores are embedded.
#![cfg(feature = "tree_flows")]

// The REAL service cores, embedded via `include!` inside feature-gated
// inline mods — `include!` is opaque to rustfmt (single-repo CI fmt
// stays green) and resolves only under the tree layout
// (exchange8-{gateway,me,clearing} checked out side by side).

#[cfg(feature = "tree_flows")]
mod submission {
    include!("../../../../exchange8-gateway/src/submission.rs");
}

#[cfg(feature = "tree_flows")]
#[allow(dead_code)] // bin-module helpers unreachable from the test crate
mod engine {
    include!("../../../../exchange8-me/src/engine.rs");
}

#[cfg(feature = "tree_flows")]
mod projector {
    include!("../../../../exchange8-clearing/src/projector.rs");
}

use exchange8_contracts::enums::{Capacity, Liquidity, OrderType, Side, Visibility};
use exchange8_contracts::events::{AddOrder, TransactionRejected};
use exchange8_contracts::order::{
    BookToken, ExchangeOrder, InstrumentKind, InstrumentRef, OrdStatus, OrdType, OrderSide, Tif,
};
use exchange8_fixed::{Price, Qty};

/// The canonical happy-path script (guarding the sim DSL itself).
/// Field names follow the harness DSL: `sym` (symbol), `px` (price).
const HAPPY_SCRIPT: &str = r#"
# maker rests on the book
D trader=MM1 sym=XL8M side=B px=101_000 qty=100 tif=PL
# taker crosses
F trader=CL1 sym=XL8M side=S px=101_000 qty=50 tif=PL
"#;

fn instrument() -> InstrumentRef {
    InstrumentRef {
        id: 1,
        symbol: "EX8A".into(),
        isin: None,
        mic: "XL8M".into(),
        currency: "CAD".into(),
        price_scale: 4,
        qty_scale: 0,
        lot_size: Qty::units(100),
        kind: InstrumentKind::Equity,
    }
}

/// Gateway-shaped order fixture (mirrors `submission.rs`'s own test
/// fixture so the flow test exercises the exact same shape).
fn gateway_order(mutate: impl FnOnce(&mut ExchangeOrder)) -> ExchangeOrder {
    let mut o = ExchangeOrder {
        env_id: "e".into(),
        gw_order_id: 0,
        cl_ord_id: "C1".into(),
        orig_cl_ord_id: None,
        account: "ACCT".into(),
        client_comp_id: "MEMBER".into(),
        on_behalf_comp_id: None,
        user_id: None,
        connection_id: 7,
        instrument: instrument(),
        side: OrderSide::Buy,
        qty: Qty::units(100),
        min_qty: None,
        price: Some(Price::equity(10_000)),
        ord_type: OrdType::Limit,
        tif: Tif::Day,
        peg_type: None,
        peg_difference: None,
        peg_ebbo: false,
        visibility: Visibility::Display,
        max_floor: None,
        capacity: Capacity::Agency,
        anonymous: false,
        bypass: false,
        price_protection: None,
        post_only: false,
        book: BookToken('*'),
        strategy: vec![],
        stp_key: None,
        stp_action: None,
        cross: None,
        ex_destination: None,
        ord_status: OrdStatus::New,
        cum_qty: Qty::ZERO,
        leaves_qty: Qty::units(100),
        avg_px: None,
        timestamp_us: 0,
    };
    mutate(&mut o);
    o
}

/// ME add-order fixture — identical to `engine.rs`'s own test fixture.
fn add(order_id: u64, side: Side, px: i128, qty: u64, connection: u16) -> AddOrder {
    AddOrder {
        price: Price::equity(px),
        order_id,
        sub_order_id: 0,
        instrument_id: 1,
        qty: Qty::units(qty),
        connection_id: connection,
        side,
        order_type: OrderType::Limit,
        visibility: Visibility::Display,
        capacity: Capacity::Agency,
        account: if side == Side::Bid { "MM1" } else { "CL1" }.into(),
        stp_key: None,
        stp_action: None,
        customer_id: 1,
        book_id: 0,
        anonymous: false,
        price_protection: None,
        max_floor: Qty::ZERO,
        strategy: vec![b'*'],
    }
}

/// Happy path end-to-end: Gateway routes → ME matches. The aggressor is
/// a BID into a resting ASK (the engine matches aggressive buys against
/// the ask book — mirrors `engine.rs`'s price_time test).
#[test]
fn gateway_submit_me_match_happy_path() {
    // 1 — GATEWAY: a plain limit routes to the matching engine.
    let route = submission::route_order(&gateway_order(|_| {}));
    assert!(matches!(route, Ok(submission::Route::MatchingEngine)));

    // 2 — ME: two asks rest, aggressive bid crosses the first fully.
    let mut me = engine::MatchingEngine::new();
    me.add_order(&add(1, Side::Ask, 10_100, 100, 7));
    me.add_order(&add(2, Side::Ask, 10_100, 100, 7));
    me.drain_events();
    me.add_order(&add(9, Side::Bid, 10_100, 100, 8));

    let events = me.drain_events();
    let executed: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            engine::MeEvent::Executed(x) => Some(x.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(executed.len(), 2, "one cross = two side events");
}

/// Strict invariant: every ME execution emits exactly TWO side events
/// sharing one trade_id (Removed taker + Added maker), and the clearing
/// trade projector yields exactly two rows for them.
#[test]
fn execution_emits_two_sides_and_projector_yields_two_rows() {
    // Engine-exact cross: two asks rest, aggressive bid crosses the first.
    let mut me = engine::MatchingEngine::new();
    me.add_order(&add(1, Side::Ask, 10_100, 100, 7));
    me.drain_events();
    me.add_order(&add(2, Side::Bid, 10_100, 100, 8));

    let events = me.drain_events();
    eprintln!("INVARIANT EVENTS: {}", events.len());
    for e in &events {
        eprintln!("{e:?}");
    }
    let executed: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            engine::MeEvent::Executed(x) => Some(x.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(executed.len(), 2, "one cross = two side events");
    assert_eq!(executed[0].trade_id, executed[1].trade_id);
    let liqs: Vec<_> = executed.iter().map(|x| x.liquidity).collect();
    assert!(
        liqs.contains(&Liquidity::Added) && liqs.contains(&Liquidity::Removed),
        "one Added + one Removed side event"
    );

    let mut tp = projector::TradeProjector::new();
    for x in &executed {
        tp.on_executed(x);
    }
    let rows = tp.rows();
    assert_eq!(rows.len(), 2, "two per-side trade rows");
    assert_eq!(rows[0].trade_id, rows[1].trade_id);
    assert!(
        tp.anomalies.is_empty(),
        "no pair anomalies: {:#?}",
        tp.anomalies
    );
}

/// Reject path: crossing into an empty book → ME Rejected → the clearing
/// order projector renders the standard NewPending → Cancelled pair.
#[test]
fn reject_projects_newpending_cancelled_pair() {
    // An aggressive order into an EMPTY book: no liquidity → the order
    // books (limit), it does NOT reject. A true reject: bid into a
    // HALTED instrument. Use status reject for the pipeline test.
    let mut me = engine::MatchingEngine::new();
    me.set_instrument_status(1, exchange8_contracts::enums::InstrumentStatus::Halted);
    me.add_order(&add(9, Side::Bid, 10_100, 25, 8));
    let events = me.drain_events();

    let rejected: Vec<TransactionRejected> = events
        .iter()
        .filter_map(|e| match e {
            engine::MeEvent::Rejected(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(rejected.len(), 1, "cross into empty book must reject");

    let mut op = projector::OrderProjector::new();
    // `TransactionRejected` carries no timestamp; the projector's reject
    // path takes the caller-supplied ts (the shell passes event time).
    op.on_rejected(9, 1, 8, 0);
    let rows = op.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].update_type, projector::UpdateType::NewPending);
    assert_eq!(rows[1].update_type, projector::UpdateType::Cancelled);
    assert_eq!(rows[0].order_id, rows[1].order_id);
}

/// Gateway identity/quantity rejects (the pre-ME policy layer).
#[test]
fn gateway_policy_rejects_malformed_submissions() {
    assert!(matches!(
        submission::route_order(&gateway_order(|o| {
            o.account = String::new();
        })),
        Err(submission::SubmitReject::MissingIdentity)
    ));
    assert!(matches!(
        submission::route_order(&gateway_order(|o| {
            o.qty = Qty::ZERO;
        })),
        Err(submission::SubmitReject::ZeroQty)
    ));
    assert!(matches!(
        submission::route_order(&gateway_order(|o| {
            o.price = None;
        })),
        Err(submission::SubmitReject::LimitWithoutPrice)
    ));
}

/// The sim DSL parses the canonical script (harness stays 1:1 with the
/// platform spec verbs).
#[test]
fn sim_harness_parses_happy_script() {
    let actions = exchange8_sim_harness::parse_script(HAPPY_SCRIPT).unwrap();
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        actions[0],
        exchange8_sim_harness::Action::Deposit(_)
    ));
    assert!(matches!(
        actions[1],
        exchange8_sim_harness::Action::Enter(_)
    ));
}
