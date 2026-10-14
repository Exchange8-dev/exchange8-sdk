// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Within-level allocation policies (matching-engine design §4.4).
//!
//! This module is the decision core of the book: it answers "how does an
//! incoming order consume the resting liquidity of one price level". The
//! policy is a per-contract property (`exchange8_contract.allocation_method`),
//! selected by the engine per instrument:
//!
//! - `fifo` (default): strict price-time — consume a FIFO prefix of the
//!   level (orders 1..k fully, k+1 partially, later untouched).
//! - `pro_rata_round_lot`: round-lot shares ∝ each resting order's volume
//!   first; the indivisible remainder by largest-remainder, ties by arrival
//!   (commodity-futures convention).
//! - `pro_rata_fifo_remainder`: pro-rata core; the indivisible remainder
//!   allocates FIFO.
//!
//! The allocator is pure: it takes a snapshot of a level (ordered triples
//! of `(order_id, qty, seq)`) and an incoming qty, and returns the per-order
//! fill list. The engine applies the fills via `SideBook::reduce` /
//! `remove` exactly like the FIFO path. All policies guarantee the same
//! invariants: Σ fills == min(incoming, Σ resting), no oversell
//! (fill_i ≤ resting_i), deterministic output for identical input.
//!
//! See `crates/book/DESIGN.md` §2 (allocation pass) and §4 (API surface)
//! for the module's role in the crate architecture.

use std::collections::BTreeMap;

/// Per-contract allocation method (mirrors the SQL CHECK values).
///
/// The contract master (`exchange8_contract.allocation_method`) selects how
/// an incoming order consumes resting liquidity at one price level; the
/// engine reads it per instrument and passes the value to [`allocate`].
/// The set is closed: the string round-trip (`from_db`/`as_str`) covers
/// exactly the CHECK values of the schema, and any unknown input yields
/// `None` rather than a default.
///
/// # Examples
/// ```
/// use exchange8_book::AllocationMethod;
///
/// let m = AllocationMethod::from_db("pro_rata_round_lot").unwrap();
/// assert_eq!(m, AllocationMethod::ProRataRoundLot);
/// assert_eq!(m.as_str(), "pro_rata_round_lot");
/// assert!(AllocationMethod::from_db("size_priority").is_none());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AllocationMethod {
    /// Strict price-time prefix consumption (the equity default).
    Fifo,
    /// Pro-rata shares; indivisible remainder by largest fractional
    /// remainder, ties by arrival (commodity-futures convention).
    ProRataRoundLot,
    /// Pro-rata shares; indivisible remainder awarded FIFO (hybrid
    /// products).
    ProRataFifoRemainder,
}

impl AllocationMethod {
    /// Parse a contract-master column value into a method.
    ///
    /// Returns `None` for any string outside the schema's CHECK set — a
    /// malformed row surfaces as a provisioning error, never as a silent
    /// default policy.
    ///
    /// # Examples
    /// ```
    /// # use exchange8_book::AllocationMethod;
    /// assert_eq!(
    ///     AllocationMethod::from_db("fifo"),
    ///     Some(AllocationMethod::Fifo)
    /// );
    /// assert_eq!(AllocationMethod::from_db("FIFO"), None);
    /// ```
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "fifo" => Some(AllocationMethod::Fifo),
            "pro_rata_round_lot" => Some(AllocationMethod::ProRataRoundLot),
            "pro_rata_fifo_remainder" => Some(AllocationMethod::ProRataFifoRemainder),
            _ => None,
        }
    }

    /// Render the method as its contract-master column value (exact
    /// inverse of [`from_db`]).
    ///
    /// [`from_db`]: AllocationMethod::from_db
    pub fn as_str(self) -> &'static str {
        match self {
            AllocationMethod::Fifo => "fifo",
            AllocationMethod::ProRataRoundLot => "pro_rata_round_lot",
            AllocationMethod::ProRataFifoRemainder => "pro_rata_fifo_remainder",
        }
    }
}

/// One resting order's contribution to a level, as seen by the allocator.
///
/// The engine snapshots a level into these value triples so allocation is
/// a pure function with no reference into the live book; the engine
/// applies the returned fills through the book's own primitives.
///
/// # Examples
/// ```
/// use exchange8_book::{AllocationMethod, RestingQty, allocate};
///
/// let level = vec![RestingQty { order_id: 1, qty: 60, seq: 1 }];
/// let fills = allocate(AllocationMethod::Fifo, &level, 100);
/// assert_eq!(fills.len(), 1);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestingQty {
    /// Exchange-wide resting-order id this snapshot row represents.
    pub order_id: u64,
    /// Quantity available to fill at this level (visible portion).
    pub qty: u64,
    /// Arrival sequence — FIFO position and remainder tie-break.
    pub seq: u64,
}

/// One allocation decision: a resting-order id and the granted quantity.
///
/// Fills are the allocator's output contract; the engine applies each fill
/// through [`crate::SideBook::reduce`] (and `remove` on full drains). All
/// policies guarantee Σ fills = min(incoming, Σ resting) and fill ≤
/// resting per order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fill {
    /// Resting-order id granted the quantity.
    pub order_id: u64,
    /// Granted (exact integer) quantity — never above the resting cap.
    pub qty: u64,
}

/// Allocate `incoming` across the level under the given policy.
///
/// Orders arrive in FIFO order (front first). Returns per-order fills in a
/// deterministic order (arrival for Fifo; allocation-applied order for
/// pro-rata policies, sorted by arrival seq).
///
/// Guarantees for every policy: Σ fills = min(incoming, Σ resting qty);
/// each fill ≤ its order's qty; identical inputs yield identical outputs
/// (pure integer math, no floats, no wall-clock inputs).
///
/// # Examples
/// ```
/// use exchange8_book::{AllocationMethod, Fill, RestingQty, allocate};
///
/// let level = vec![
///     RestingQty { order_id: 1, qty: 60, seq: 1 },
///     RestingQty { order_id: 2, qty: 40, seq: 2 },
/// ];
/// let fills = allocate(AllocationMethod::ProRataRoundLot, &level, 50);
/// assert_eq!(
///     fills,
///     vec![Fill { order_id: 1, qty: 30 }, Fill { order_id: 2, qty: 20 }]
/// );
/// ```
pub fn allocate(method: AllocationMethod, level: &[RestingQty], incoming: u64) -> Vec<Fill> {
    match method {
        AllocationMethod::Fifo => allocate_fifo(level, incoming),
        AllocationMethod::ProRataRoundLot => allocate_pro_rata(level, incoming, false),
        AllocationMethod::ProRataFifoRemainder => allocate_pro_rata(level, incoming, true),
    }
}

/// Strict FIFO prefix consumption (the equity default).
fn allocate_fifo(level: &[RestingQty], incoming: u64) -> Vec<Fill> {
    let mut out = Vec::new();
    let mut need = incoming;
    for r in level {
        if need == 0 {
            break;
        }
        let take = need.min(r.qty);
        if take > 0 {
            out.push(Fill {
                order_id: r.order_id,
                qty: take,
            });
            need -= take;
        }
    }
    out
}

/// Pro-rata within a level.
///
/// Round-lot shares: each order's exact share is
/// `qty × incoming / total`; the floor of that is granted outright.
/// Remainder distribution: the indivisible leftover units
/// (`incoming − Σ floors`) go by largest fractional remainder (ties by
/// arrival seq). `fifo_remainder = true` switches the remainder
/// distribution to plain FIFO.
///
/// An incoming qty ≥ total level volume sweeps everything (each order
/// fills its full qty — pro-rata degenerates to full consumption).
fn allocate_pro_rata(level: &[RestingQty], incoming: u64, fifo_remainder: bool) -> Vec<Fill> {
    if level.is_empty() || incoming == 0 {
        return Vec::new();
    }
    let total: u64 = level.iter().map(|r| r.qty).sum();
    if total == 0 {
        return Vec::new();
    }
    // Full sweep: everything fills.
    if incoming >= total {
        return level
            .iter()
            .filter(|r| r.qty > 0)
            .map(|r| Fill {
                order_id: r.order_id,
                qty: r.qty,
            })
            .collect();
    }

    // Round-lot shares (∝ qty). i128 math: qty,incoming ≤ u64 range and
    // the product of two u64s fits i128 without overflow.
    let mut fills: BTreeMap<u64, u64> = BTreeMap::new(); // order_id → granted
                                                         // (order_id, fractional remainder numerator, seq) — remainder is
                                                         // (qty × incoming) mod total, the part of the exact share above its floor.
    let mut fractional: Vec<(u64, i128, u64)> = Vec::new();
    let mut granted: u64 = 0;
    for r in level {
        if r.qty == 0 {
            continue;
        }
        let prod = r.qty as i128 * incoming as i128;
        let floor = (prod / total as i128) as u64;
        fills.insert(r.order_id, floor);
        granted += floor;
        fractional.push((r.order_id, prod % total as i128, r.seq));
    }

    // distribute `leftover` units — one per order per round,
    // cycling the ranking until exhausted (standard largest-remainder
    // semantics: each round grants at most 1 unit to an order, capped by
    // its unfilled qty).
    let mut leftover = incoming - granted;
    if fifo_remainder {
        // FIFO: oldest orders get the extra units first.
        let mut by_seq: Vec<&(u64, i128, u64)> = fractional.iter().collect();
        by_seq.sort_by_key(|(_, _, seq)| *seq);
        let order: Vec<u64> = by_seq.iter().map(|(id, _, _)| *id).collect();
        distribute_rounds(&mut fills, &order, level, &mut leftover);
    } else {
        // Largest remainder first; ties by arrival seq.
        fractional.sort_by(|a, b| (b.1, a.2).cmp(&(a.1, b.2)));
        let order: Vec<u64> = fractional.iter().map(|(id, _, _)| *id).collect();
        distribute_rounds(&mut fills, &order, level, &mut leftover);
    }

    // Emit in arrival order with zero-qty entries stripped.
    level
        .iter()
        .filter(|r| fills.get(&r.order_id).copied().unwrap_or(0) > 0)
        .map(|r| Fill {
            order_id: r.order_id,
            qty: fills[&r.order_id],
        })
        .collect()
}

/// Grant at most 1 unit per order per pass over `order`, cycling until
/// `leftover` is exhausted or no order has room.
fn distribute_rounds(
    fills: &mut BTreeMap<u64, u64>,
    order: &[u64],
    level: &[RestingQty],
    leftover: &mut u64,
) {
    if order.is_empty() || *leftover == 0 {
        return;
    }
    let caps: BTreeMap<u64, u64> = level.iter().map(|r| (r.order_id, r.qty)).collect();
    loop {
        if *leftover == 0 {
            return;
        }
        let mut progressed = false;
        for id in order {
            if *leftover == 0 {
                return;
            }
            let cap = caps[id];
            let already = fills[id];
            if already < cap {
                *fills.get_mut(id).unwrap() += 1;
                *leftover -= 1;
                progressed = true;
            }
        }
        if !progressed {
            return; // nobody has room (cannot happen when incoming < total)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(items: &[(u64, u64, u64)]) -> Vec<RestingQty> {
        items
            .iter()
            .map(|(id, qty, seq)| RestingQty {
                order_id: *id,
                qty: *qty,
                seq: *seq,
            })
            .collect()
    }

    fn total(fills: &[Fill]) -> u64 {
        fills.iter().map(|f| f.qty).sum()
    }

    #[test]
    fn fifo_is_strict_prefix() {
        let l = level(&[(1, 100, 1), (2, 100, 2), (3, 100, 3)]);
        // 150 consumes order 1 fully, order 2 by 50; order 3 untouched.
        let fills = allocate(AllocationMethod::Fifo, &l, 150);
        assert_eq!(
            fills,
            vec![
                Fill {
                    order_id: 1,
                    qty: 100
                },
                Fill {
                    order_id: 2,
                    qty: 50
                }
            ]
        );
    }

    #[test]
    fn fifo_full_sweep() {
        let l = level(&[(1, 100, 1), (2, 100, 2)]);
        let fills = allocate(AllocationMethod::Fifo, &l, 500);
        assert_eq!(total(&fills), 200); // capped at level volume
        assert_eq!(fills.len(), 2);
    }

    #[test]
    fn pro_rata_exact_shares() {
        // 50 incoming across 60/40 → exact 30/20, no remainder.
        let l = level(&[(1, 60, 1), (2, 40, 2)]);
        let fills = allocate(AllocationMethod::ProRataRoundLot, &l, 50);
        assert_eq!(
            fills,
            vec![
                Fill {
                    order_id: 1,
                    qty: 30
                },
                Fill {
                    order_id: 2,
                    qty: 20
                }
            ]
        );
    }

    #[test]
    fn pro_rata_largest_remainder_breaks_ties() {
        // 100 incoming across 60/40/30/20 (total 150): exact shares
        // 40/26.67/20/13.33 → floors 40/26/20/13 = 99; 1 unit left.
        // Fractional remainders: order 2 (2/3) largest → gets it.
        let l = level(&[(1, 60, 1), (2, 40, 2), (3, 30, 3), (4, 20, 4)]);
        let fills = allocate(AllocationMethod::ProRataRoundLot, &l, 100);
        assert_eq!(total(&fills), 100);
        let f1 = fills.iter().find(|f| f.order_id == 1).unwrap();
        assert_eq!(f1.qty, 40); // exact share, no remainder needed
        let f2 = fills.iter().find(|f| f.order_id == 2).unwrap();
        assert_eq!(f2.qty, 27); // 26 floor + 1 largest-remainder (frac 2/3)
        let f3 = fills.iter().find(|f| f.order_id == 3).unwrap();
        assert_eq!(f3.qty, 20);
        let f4 = fills.iter().find(|f| f.order_id == 4).unwrap();
        assert_eq!(f4.qty, 13);
    }

    #[test]
    fn pro_rata_remainder_tie_breaks_by_arrival() {
        // 2 units left after floors; orders 10 and 11 tie on fractional
        // remainder (7/8 each) → arrival order (seq) wins.
        let l = level(&[(10, 3, 5), (11, 3, 6), (12, 2, 7)]); // total 8
        let fills = allocate(AllocationMethod::ProRataRoundLot, &l, 5);
        assert_eq!(total(&fills), 5);
        // exact: 15/8/15/8/10/8 → floors 1/1/1 = 3; 2 left; fracs
        // 7/8, 7/8, 2/8 → both 10 and 11 get +1 (ties by seq: 10 first).
        let f10 = fills.iter().find(|f| f.order_id == 10).unwrap();
        let f11 = fills.iter().find(|f| f.order_id == 11).unwrap();
        let f12 = fills.iter().find(|f| f.order_id == 12).unwrap();
        assert_eq!((f10.qty, f11.qty, f12.qty), (2, 2, 1));
    }

    #[test]
    fn pro_rata_full_sweep_fills_everything() {
        // Incoming ≥ total: every order fills fully (degenerate case).
        let l = level(&[(1, 60, 1), (2, 40, 2)]);
        let fills = allocate(AllocationMethod::ProRataRoundLot, &l, 200);
        assert_eq!(
            fills,
            vec![
                Fill {
                    order_id: 1,
                    qty: 60
                },
                Fill {
                    order_id: 2,
                    qty: 40
                }
            ]
        );
    }

    #[test]
    fn pro_rata_never_oversells() {
        // Tiny incoming across lopsided levels: floors are 0, remainder
        // lands on one order only, never above its qty.
        let l = level(&[(1, 1000, 1), (2, 1, 2)]);
        let fills = allocate(AllocationMethod::ProRataRoundLot, &l, 1);
        assert_eq!(total(&fills), 1);
        for f in &fills {
            let cap = l.iter().find(|r| r.order_id == f.order_id).unwrap().qty;
            assert!(f.qty <= cap);
        }
    }

    #[test]
    fn pro_rata_fifo_remainder_uses_arrival() {
        // Same tie case as above, but the 2 leftover units go FIFO:
        // oldest (10) then (11) — same result here; use a case where
        // largest-remainder differs from FIFO.
        // total 100, incoming 30: exact 30/20/… floors 30/20; 0 leftover is
        // trivial — construct a real one:
        // orders 3/3/3/1 (total 10), incoming 4:
        // exact 1.2/1.2/1.2/0.4 → floors 1/1/1/0 = 3; 1 left.
        // largest-remainder: first of the .2s by seq (order 1).
        // FIFO remainder: also order 1 (oldest) — need a case where fracs
        // disagree with seq: qty 1 (seq 1) vs qty 9 (seq 2), incoming 5:
        // exact .5/4.5 → floors 0/4; 1 left; fracs .5/.5 tie → seq: order 1.
        // FIFO: order 1. Same again — invert: qty 9 (seq 1), qty 1 (seq 2):
        // exact 4.5/.5 → floors 4/0, 1 left; tie .5/.5 → largest-rem picks
        // seq 1 (order 1) → 5/0; FIFO picks order 1 too → 5/0. The
        // distinguishing case: three orders 2/2/1 (seq 1,2,3), incoming 3:
        // exact .857/.857/.429 → floors 0/0/0, 3 left. Largest-rem: the two
        // .857s (orders 1,2) → 2/1/0... wait, 3 units: orders 1,2 get 1
        // each (fracs .857), then order 3 (.429) gets the last → 1/1/1.
        // FIFO: orders 1,2,3 in seq order → 1/1/1. Both equal — the tie
        // break genuinely coincides here; assert conservation instead.
        let l = level(&[(1, 2, 1), (2, 2, 2), (3, 1, 3)]);
        let fills = allocate(AllocationMethod::ProRataFifoRemainder, &l, 3);
        assert_eq!(total(&fills), 3);
        // The distinguishing case for the two pro-rata variants:
        // qty 4 (seq 2), qty 3 (seq 1), qty 3 (seq 3), total 10, incoming 6:
        // exact 2.4/1.8/1.8 → floors 2/1/1 = 4; 2 left; fracs .4/.8/.8.
        // Largest-rem: orders 2,3 (fracs .8, tie → seq: order 2 first)
        // → order 2 +1, order 3 +1 → 2/2/2.
        // FIFO-rem: oldest = order 2 (seq 1) → +1 (room 2), then order 1
        // (seq 2) → +1 → 3/2/1.
        let l2 = level(&[(1, 4, 2), (2, 3, 1), (3, 3, 3)]);
        let lr = allocate(AllocationMethod::ProRataRoundLot, &l2, 6);
        let fr = allocate(AllocationMethod::ProRataFifoRemainder, &l2, 6);
        // Largest-remainder: order2 and order3 top up.
        let g = |f: &[Fill], id: u64| {
            f.iter()
                .find(|x| x.order_id == id)
                .map(|x| x.qty)
                .unwrap_or(0)
        };
        assert_eq!((g(&lr, 1), g(&lr, 2), g(&lr, 3)), (2, 2, 2));
        // FIFO remainder: oldest orders top up.
        assert_eq!((g(&fr, 1), g(&fr, 2), g(&fr, 3)), (3, 2, 1));
    }

    #[test]
    fn conservation_all_policies_all_shapes() {
        let shapes = vec![
            vec![(1u64, 100u64, 1u64), (2, 100, 2), (3, 100, 3)],
            vec![(1, 60, 1), (2, 40, 2)],
            vec![(1, 7, 3), (2, 13, 1), (3, 29, 2), (4, 1, 4)],
            vec![(1, 1000, 1), (2, 1, 2), (3, 499, 3)],
        ];
        for s in shapes {
            let l = level(&s);
            let lv_total: u64 = l.iter().map(|r| r.qty).sum();
            for &incoming in &[1u64, 2, 3, 7, 13, 50, 100, 499, 500, 1000, 1500, 2000] {
                for &m in &[
                    AllocationMethod::Fifo,
                    AllocationMethod::ProRataRoundLot,
                    AllocationMethod::ProRataFifoRemainder,
                ] {
                    let fills = allocate(m, &l, incoming);
                    let expect = incoming.min(lv_total);
                    assert_eq!(
                        total(&fills),
                        expect,
                        "method {:?} incoming {} level {:?}",
                        m,
                        incoming,
                        s
                    );
                    for f in &fills {
                        let cap = l.iter().find(|r| r.order_id == f.order_id).unwrap().qty;
                        assert!(f.qty <= cap, "oversell: {:?} incoming {}", fills, incoming);
                    }
                }
            }
        }
    }

    #[test]
    fn db_round_trip() {
        assert_eq!(
            AllocationMethod::from_db("fifo"),
            Some(AllocationMethod::Fifo)
        );
        assert_eq!(
            AllocationMethod::from_db("pro_rata_round_lot"),
            Some(AllocationMethod::ProRataRoundLot)
        );
        assert_eq!(
            AllocationMethod::from_db("pro_rata_fifo_remainder"),
            Some(AllocationMethod::ProRataFifoRemainder)
        );
        assert_eq!(AllocationMethod::from_db("bogus"), None);
        assert_eq!(AllocationMethod::Fifo.as_str(), "fifo");
    }

    #[test]
    fn empty_level_and_zero_incoming() {
        let l: Vec<RestingQty> = Vec::new();
        assert!(allocate(AllocationMethod::Fifo, &l, 100).is_empty());
        let l2 = level(&[(1, 100, 1)]);
        assert!(allocate(AllocationMethod::ProRataRoundLot, &l2, 0).is_empty());
    }
}
