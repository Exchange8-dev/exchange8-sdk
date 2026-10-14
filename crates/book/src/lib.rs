// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 price-level order-book primitives.
//!
//! This crate is the in-memory book of the exchange8 vertical: it holds
//! resting orders, orders them best-price-first (visible before hidden at
//! an equal price), FIFO by arrival within each level, and hands the
//! matching engine deterministic within-level allocation policies. It is a
//! pure domain library: no I/O, no threads, no persistence — consumers own
//! the lifecycle and the single-writer schedule. The authoritative consumer
//! is the matching engine (Ex8:ME); market-data mirrors and the simulation
//! harness consume the same structures so displayed depth and simulation
//! behavior cannot drift from the matching truth.
//!
//! # Quick start
//!
//! ```
//! use exchange8_book::{allocate, AllocationMethod, RestingOrder, RestingQty, SideBook};
//! use exchange8_contracts::Side;
//! use exchange8_fixed::{Price, Qty};
//!
//! let mut bids = SideBook::new(Side::Bid);
//! bids.insert(RestingOrder {
//!     order_id: 1,
//!     sub_order_id: 0,
//!     customer_id: 1,
//!     connection_id: 1,
//!     side: Side::Bid,
//!     price: Price::equity(100_000), // 10.0000 at the equity scale
//!     full_volume: Qty::units(500),
//!     remaining: Qty::units(500),
//!     visible: Qty::units(500),
//!     max_floor: Qty::units(0),
//!     iceberg_sub_order_count: 0,
//!     is_odd_lot: false,
//!     anonymous: false,
//!     stp_key: None,
//!     seq: 0, // minted by the SideBook; caller value ignored
//! });
//!
//! // Best-first depth, visible/displayable top of book:
//! let best = bids.best().unwrap();
//! assert_eq!(best.price.raw, 100_000);
//! assert_eq!(best.visible_volume(), 500);
//!
//! // Within-level allocation as a pure decision:
//! let level = vec![RestingQty { order_id: 1, qty: 500, seq: 1 }];
//! let fills = exchange8_book::allocate(AllocationMethod::Fifo, &level, 200);
//! assert_eq!(fills[0].qty, 200);
//! ```
//!
//! # Architecture
//!
//! See `crates/book/DESIGN.md` for the detailed design (data structures,
//! allocation policies, invariants, concurrency model) and
//! `crates/book/README.md` for the overview. Per-contract allocation
//! policies live in the [`alloc`] module (`fifo` default for equities,
//! pro-rata variants for derivatives/commodities); the matching-engine
//! design doc `design/sentin8-exchange8-matching-engine-technical-design.md`
//! (§4.2 book structure, §4.4 matching algorithm) defines the consuming
//! context.
//!
//! **Data-structure contract** — the hot path is sub-linear in the level
//! count L (distinct price × visibility buckets):
//!
//! | Operation | Complexity |
//! |---|---|
//! | insert | O(log L) |
//! | best() / best_visible() | O(log L) |
//! | remove by id | O(1) locate (id index) + level-local removal |
//! | front consumption | O(1) |
//! | FOK volume-at-or-better | O(L) |
//!
//! Implementation: one `BTreeMap<PriceKey, PriceLevel>` per side where the
//! key's `price_rank` is inverted for bids, so **map iteration order is
//! best-first on both sides** — the matching walk advances through the map
//! with zero rescans. Each level's FIFO queue is a `VecDeque` (tail insert,
//! front consume). An `order_id → PriceKey` index makes cancel/reduce O(1)
//! to locate.
//!
//! **Display semantics**: visible and hidden orders at the same price form
//! **separate levels**, visible first at equal price — hidden liquidity is
//! never part of displayed depth and bypass flow only touches visible levels.
//!
//! **Within-level discipline**: the book stores orders FIFO by arrival
//! sequence; the consumption policy (strict FIFO prefix vs pro-rata) is
//! selected per contract by the engine via `alloc::allocate`.

pub mod alloc;

/// Re-exports of the allocation-policy surface at the crate root, so
/// consumers reach the per-contract fill discipline without the module
/// path (`use exchange8_book::allocate`).
pub use alloc::{allocate, AllocationMethod, Fill, RestingQty};

use std::collections::{BTreeMap, VecDeque};

use exchange8_contracts::Side;
use exchange8_fixed::{Price, Qty};
use serde::{Deserialize, Serialize};

/// One resting order with its matching-relevant identity and state.
///
/// The book stores these by value inside its level queues; the matching
/// engine constructs them from validated inbound orders and reads them back
/// during the fill walk. All integer identities are small, typed fields so
/// a mirrored book can be rebuilt from a serialized frame without
/// re-resolving external references.
///
/// # Examples
///
/// ```
/// use exchange8_book::{RestingOrder};
/// use exchange8_contracts::Side;
/// use exchange8_fixed::{Price, Qty};
///
/// let o = RestingOrder {
///     order_id: 42,
///     sub_order_id: 0,
///     customer_id: 7,
///     connection_id: 1,
///     side: Side::Bid,
///     price: Price::equity(100_000), // 10.0000
///     full_volume: Qty::units(500),
///     remaining: Qty::units(500),
///     visible: Qty::units(100),
///     max_floor: Qty::units(100),
///     iceberg_sub_order_count: 0,
///     is_odd_lot: false,
///     anonymous: false,
///     stp_key: None,
///     seq: 0,
/// };
/// assert!(o.is_iceberg());
/// assert!(o.has_hidden_part());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestingOrder {
    /// Exchange-wide resting-order id (cancel/reduce key).
    pub order_id: u64,
    /// Matching-engine sub-order id (iceberg slice identity on trades).
    pub sub_order_id: u32,
    /// Interior customer id (time/id pairs, preferencing, wash flags).
    pub customer_id: u16,
    /// Market-connection the order arrived on (cancel-on-disconnect).
    pub connection_id: u16,
    /// Order side (bid or ask); the owning `SideBook` always agrees.
    pub side: Side,
    /// Resting (passive) price in scaled ticks.
    pub price: Price,
    /// Full order volume (iceberg total incl. reserve).
    pub full_volume: Qty,
    /// Remaining (unfilled) volume incl. hidden.
    pub remaining: Qty,
    /// Visible (display) quantity currently resting.
    pub visible: Qty,
    /// Iceberg max display; `raw == 0` = not an iceberg.
    pub max_floor: Qty,
    /// Incremented on every iceberg reload (feed identity for the slice).
    pub iceberg_sub_order_count: u32,
    /// Odd-lot-only resting order (lives in the odd books, never
    /// matches board liquidity).
    pub is_odd_lot: bool,
    /// Anonymous (hidden) resting order.
    pub anonymous: bool,
    /// STP key (customer/location/connection/account).
    pub stp_key: Option<u64>,
    /// Arrival sequence (time priority within a level); minted by the
    /// `SideBook` on insert — monotone per side, caller-supplied values
    /// are overwritten.
    pub seq: u64,
}

impl RestingOrder {
    /// True when the order carries an iceberg max-display size.
    ///
    /// Icebergs rest with `visible` capped at `max_floor` and a hidden
    /// reserve in `remaining − visible`; each display reload re-enters
    /// the order at its level's tail.
    pub fn is_iceberg(&self) -> bool {
        self.max_floor.raw > 0
    }

    /// True when part of the remaining volume is currently hidden.
    ///
    /// Holds for every iceberg with an undisplayed reserve (and for any
    /// order resting with `visible == remaining.raw` when `remaining`
    /// is fully displayed — the predicate is the hidden-part test only,
    /// not the iceberg test).
    ///
    /// # Examples
    ///
    /// ```
    /// # use exchange8_book::RestingOrder;
    /// # use exchange8_contracts::Side;
    /// # use exchange8_fixed::{Price, Qty};
    /// let o = RestingOrder {
    ///     order_id: 1,
    ///     sub_order_id: 0,
    ///     customer_id: 0,
    ///     connection_id: 0,
    ///     side: Side::Ask,
    ///     price: Price::equity(10_000),
    ///     full_volume: Qty::units(1000),
    ///     remaining: Qty::units(1000),
    ///     visible: Qty::units(100),
    ///     max_floor: Qty::units(0),
    ///     iceberg_sub_order_count: 0,
    ///     is_odd_lot: false,
    ///     anonymous: false,
    ///     stp_key: None,
    ///     seq: 0,
    /// };
    /// assert!(o.has_hidden_part());
    /// assert!(o.is_iceberg() == (o.max_floor.raw > 0));
    /// ```
    pub fn has_hidden_part(&self) -> bool {
        self.visible < self.remaining
    }
}

/// BTreeMap key ordered so that map iteration == best-first on both sides.
///
/// `price_rank` inverts raw prices for bids (highest price = lowest rank)
/// and keeps them for asks (lowest price = lowest rank). The visibility
/// tiebreak places **visible levels before hidden** at the same price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct PriceKey {
    price_rank: i128,
    /// Inverted flag: `false` (visible) sorts before `true` (hidden).
    hidden: bool,
}

impl PriceKey {
    fn new(price: Price, visible: bool, side: Side) -> Self {
        let price_rank = match side {
            Side::Bid => -price.raw,
            Side::Ask => price.raw,
        };
        PriceKey {
            price_rank,
            hidden: !visible,
        }
    }
}

/// One price level with FIFO-ordered orders.
///
/// Visible and hidden orders at the same price live in distinct levels:
/// at an equal price the visible level sorts ahead of the hidden one,
/// displayed depth aggregates visible levels only, and bypass-style flows
/// touch visible levels only. A level exists only while it has orders —
/// the owning `SideBook` prunes a level whose queue drains.
///
/// # Examples
///
/// ```
/// use exchange8_book::PriceLevel;
/// use exchange8_fixed::Price;
///
/// let mut lvl = PriceLevel::new(Price::equity(10_000), true);
/// assert!(lvl.is_empty());
/// assert_eq!(lvl.visible_volume(), 0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    /// Level price (identical for every order in the deque).
    pub price: Price,
    /// This level holds displayable (`true`) or hidden (`false`) orders.
    pub visible: bool,
    /// FIFO by arrival (`seq`) — front = oldest = next to fill.
    pub orders: VecDeque<RestingOrder>,
}

impl PriceLevel {
    /// Create an empty level at `price` with the given visibility.
    pub fn new(price: Price, visible: bool) -> Self {
        PriceLevel {
            price,
            visible,
            orders: VecDeque::new(),
        }
    }

    /// Total visible volume on this level (sum of order display state).
    pub fn visible_volume(&self) -> u64 {
        self.orders.iter().map(|o| o.visible.raw).sum()
    }

    /// Total remaining volume incl. hidden iceberg reserve.
    pub fn total_remaining(&self) -> u64 {
        self.orders.iter().map(|o| o.remaining.raw).sum()
    }

    /// True when no order rests on this level.
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
}

/// One side of a book (bids or asks), best-first.
///
/// Iteration order (BTreeMap values) is best-first: bids descending by
/// price, asks ascending — with visible-before-hidden at equal prices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideBook {
    side: Side,
    /// Best-first ordered levels (BTreeMap keyed by PriceKey).
    levels: BTreeMap<PriceKey, PriceLevel>,
    /// order_id → price key (O(1) cancel/reduce location).
    index: std::collections::HashMap<u64, PriceKey>,
    seq_counter: u64,
}

impl SideBook {
    /// Create an empty side book for `side` (iteration best-first from
    /// the first insert; sequence minting starts fresh per side book).
    pub fn new(side: Side) -> Self {
        SideBook {
            side,
            levels: BTreeMap::new(),
            index: std::collections::HashMap::new(),
            seq_counter: 0,
        }
    }

    /// The market side this book holds (bid or ask).
    pub fn side(&self) -> Side {
        self.side
    }

    /// True when `a` is strictly better than `b` on this side.
    fn is_better(&self, a: Price, b: Price) -> bool {
        match self.side {
            Side::Bid => a.raw > b.raw,
            Side::Ask => a.raw < b.raw,
        }
    }

    /// Insert a resting order (FIFO tail) at its (price, visibility).
    ///
    /// Visibility derives from the order's `visible` qty (`0` = hidden):
    /// the order joins the visible or hidden level at its price — hidden
    /// and visible orders at one price never merge into one level. The
    /// arrival sequence (`seq`) is minted here as the next monotone value,
    /// so a caller's `seq` never influences time priority; an order that
    /// must re-enter after a priority-loss re-inserted at the tail by
    /// being removed and inserted again (iceberg reload, replace).
    ///
    /// Complexity: O(log L) for the level map + O(1) tail push and index
    /// update.
    ///
    /// # Examples
    ///
    /// ```
    /// use exchange8_book::{RestingOrder, SideBook};
    /// use exchange8_contracts::{Side, Visibility};
    /// use exchange8_fixed::{Price, Qty};
    ///
    /// let mut sb = SideBook::new(Side::Ask);
    /// let o = RestingOrder {
    ///     order_id: 1,
    ///     sub_order_id: 0,
    ///     customer_id: 0,
    ///     connection_id: 0,
    ///     side: Side::Ask,
    ///     price: Price::equity(10_000),
    ///     full_volume: Qty::units(100),
    ///     remaining: Qty::units(100),
    ///     visible: Qty::units(100),
    ///     max_floor: Qty::units(0),
    ///     iceberg_sub_order_count: 0,
    ///     is_odd_lot: false,
    ///     anonymous: false,
    ///     stp_key: None,
    ///     seq: 0,
    /// };
    /// sb.insert(o);
    /// assert!(!sb.is_empty());
    /// assert_eq!(sb.best().unwrap().price.raw, 10_000);
    /// ```
    pub fn insert(&mut self, order: RestingOrder) {
        let visible = order.visible.raw > 0;
        self.seq_counter += 1;
        let mut order = order;
        order.seq = self.seq_counter;
        let key = PriceKey::new(order.price, visible, self.side);
        self.index.insert(order.order_id, key);
        self.levels
            .entry(key)
            .or_insert_with(|| PriceLevel::new(order.price, visible))
            .orders
            .push_back(order);
    }

    /// Best (front) level, if any. O(log L).
    ///
    /// The map-front lookup follows key order, so this is the
    /// execution-priority level on this side: the highest bid or the
    /// lowest ask, with the visible level ahead of hidden at equal price.
    pub fn best(&self) -> Option<&PriceLevel> {
        self.levels.values().next()
    }

    /// Best non-empty visible level (skips hidden levels).
    ///
    /// This is the displayed top-of-book: hidden levels never surface,
    /// even when they hold the strictly best price. Price still dominates
    /// the walk — a hidden better-price level is passed, not traded
    /// through — so the first visible non-empty level may sit behind it.
    pub fn best_visible(&self) -> Option<&PriceLevel> {
        self.levels.values().find(|l| l.visible && !l.is_empty())
    }

    /// Levels best-first (snapshot; O(L)). Clones the levels for callers
    /// that need to own or serialize the depth; prefer [`iter_levels`]
    /// on the hot path.
    ///
    /// [`iter_levels`]: SideBook::iter_levels
    pub fn levels(&self) -> Vec<PriceLevel> {
        self.levels.values().cloned().collect()
    }

    /// Iterate levels best-first without cloning (borrowing depth walk,
    /// the hot-path read primitive).
    pub fn iter_levels(&self) -> impl Iterator<Item = &PriceLevel> {
        self.levels.values()
    }

    /// True when no order rests on any level of this side (levels may
    /// still exist as empty if not pruned — see [`remove`]/[`prune`]).
    ///
    /// [`remove`]: SideBook::remove
    /// [`prune`]: SideBook::prune
    pub fn is_empty(&self) -> bool {
        self.levels.values().all(|l| l.is_empty())
    }

    /// Remove an order by id, returning it; prunes its level when empty.
    ///
    /// O(1) locate via the id index; removal from the level's deque is O(1)
    /// at the front/back (the matching hot path) and level-local otherwise.
    /// Returns `None` when the id is unknown, leaving the book unmodified,
    /// so callers surface a not-found rather than panicking.
    ///
    /// # Examples
    ///
    /// ```
    /// # use exchange8_book::{RestingOrder, SideBook};
    /// # use exchange8_contracts::{Side, Visibility};
    /// # use exchange8_fixed::{Price, Qty};
    /// # fn order(id: u64, qty: u64) -> RestingOrder {
    /// #     RestingOrder {
    /// #         order_id: id, sub_order_id: 0, customer_id: 0,
    /// #         connection_id: 0, side: Side::Ask,
    /// #         price: Price::equity(10_000), full_volume: Qty::units(qty),
    /// #         remaining: Qty::units(qty), visible: Qty::units(qty),
    /// #         max_floor: Qty::units(0), iceberg_sub_order_count: 0,
    /// #         is_odd_lot: false, anonymous: false, stp_key: None,
    /// #         seq: 0,
    /// #     }
    /// # }
    /// let mut sb = SideBook::new(Side::Ask);
    /// sb.insert(order(1, 100));
    /// assert_eq!(sb.remove(1).unwrap().order_id, 1);
    /// assert!(sb.remove(1).is_none());
    /// assert!(sb.best().is_none()); // level self-pruned
    /// ```
    pub fn remove(&mut self, order_id: u64) -> Option<RestingOrder> {
        let key = self.index.remove(&order_id)?;
        let level = self.levels.get_mut(&key)?;
        let removed = level
            .orders
            .iter()
            .position(|o| o.order_id == order_id)
            .and_then(|pos| level.orders.remove(pos))?;
        if level.is_empty() {
            self.levels.remove(&key);
        }
        Some(removed)
    }

    /// Prune empty levels (API parity; `remove` self-prunes).
    ///
    /// `reduce` deliberately leaves an emptied level in place (its caller
    /// decides removal versus iceberg reload), so an engine that drains
    /// levels by reduce-only paths can sweep the shell levels with this.
    pub fn prune(&mut self) {
        let empty: Vec<PriceKey> = self
            .levels
            .iter()
            .filter(|(_, l)| l.is_empty())
            .map(|(k, _)| *k)
            .collect();
        for k in empty {
            self.levels.remove(&k);
        }
    }

    /// Reduce an order's remaining + visible by `by`, saturating at zero.
    ///
    /// O(1) locate via the id index. Returns `false` when the id is
    /// unknown (or its level/index pairing is torn), leaving the book
    /// unmodified. Quantity arithmetic saturates, so a fill never drives
    /// `remaining`/`visible` negative. Empty levels are pruned by the
    /// caller (`remove` handles full-drain pruning; see [`prune`] for a
    /// reduce-only sweep).
    ///
    /// # Examples
    ///
    /// ```
    /// # use exchange8_book::{RestingOrder, SideBook};
    /// # use exchange8_contracts::{Side, Visibility};
    /// # use exchange8_fixed::{Price, Qty};
    /// # fn order(id: u64, qty: u64) -> RestingOrder {
    /// #     RestingOrder {
    /// #         order_id: id, sub_order_id: 0, customer_id: 0,
    /// #         connection_id: 0, side: Side::Ask,
    /// #         price: Price::equity(10_000), full_volume: Qty::units(qty),
    /// #         remaining: Qty::units(qty), visible: Qty::units(qty),
    /// #         max_floor: Qty::units(0), iceberg_sub_order_count: 0,
    /// #         is_odd_lot: false, anonymous: false, stp_key: None,
    /// #         seq: 0,
    /// #     }
    /// # }
    /// let mut sb = SideBook::new(Side::Ask);
    /// sb.insert(order(1, 100));
    /// assert!(sb.reduce(1, 40));
    /// assert!(!sb.reduce(999, 40)); // unknown id
    /// let lvl = sb.best().unwrap();
    /// assert_eq!(lvl.orders[0].remaining.raw, 60);
    /// assert_eq!(lvl.orders[0].visible.raw, 60);
    /// ```
    pub fn reduce(&mut self, order_id: u64, by: u64) -> bool {
        let Some(key) = self.index.get(&order_id).copied() else {
            return false;
        };
        let Some(level) = self.levels.get_mut(&key) else {
            return false;
        };
        let Some(o) = level.orders.iter_mut().find(|o| o.order_id == order_id) else {
            return false;
        };
        o.remaining = Qty::units(o.remaining.raw.saturating_sub(by));
        o.visible = Qty::units(o.visible.raw.saturating_sub(by));
        true
    }

    /// The front (oldest) order of a level at `price`, if present — the
    /// O(1) pickup for the matching walk.
    ///
    /// `visible` selects the visible or hidden level at `price`; hidden
    /// and visible levels at one price are structurally distinct, so the
    /// flag is part of the level identity, not a filter.
    ///
    /// # Returns
    ///
    /// `None` when no level exists at that (price, visibility) pair; the
    /// front order otherwise — the next to fill under time priority.
    pub fn front_at(&self, price: Price, visible: bool) -> Option<&RestingOrder> {
        let key = PriceKey::new(price, visible, self.side);
        self.levels.get(&key).and_then(|l| l.orders.front())
    }

    /// Cumulative remaining volume at prices better-or-equal to `price`
    /// (FOK availability check; aggregates visible + hidden). O(L).
    ///
    /// The walk consumes best-first levels until the limit price is
    /// reached, so a strictly worse book contributes nothing. Iceberg
    /// reserve is counted — a fill-or-kill commitment is matched against
    /// all executable liquidity, not only the display.
    ///
    /// # Examples
    ///
    /// ```
    /// # use exchange8_book::{RestingOrder, SideBook};
    /// # use exchange8_contracts::{Side, Visibility};
    /// # use exchange8_fixed::{Price, Qty};
    /// # fn order(id: u64, px: i128, qty: u64) -> RestingOrder {
    /// #     RestingOrder {
    /// #         order_id: id, sub_order_id: 0, customer_id: 0,
    /// #         connection_id: 0, side: Side::Ask,
    /// #         price: Price::equity(px), full_volume: Qty::units(qty),
    /// #         remaining: Qty::units(qty), visible: Qty::units(qty),
    /// #         max_floor: Qty::units(0), iceberg_sub_order_count: 0,
    /// #         is_odd_lot: false, anonymous: false, stp_key: None,
    /// #         seq: 0,
    /// #     }
    /// # }
    /// let mut sb = SideBook::new(Side::Ask);
    /// sb.insert(order(1, 10_000, 100));
    /// sb.insert(order(2, 10_100, 200));
    /// sb.insert(order(3, 10_200, 400));
    /// // Bids invert the rank: for Side::Bid, "better" is a higher price.
    /// assert_eq!(sb.volume_at_or_better(Price::equity(10_100)), 300);
    /// assert_eq!(sb.volume_at_or_better(Price::equity(9_900)), 0);
    /// ```
    pub fn volume_at_or_better(&self, price: Price) -> u64 {
        self.levels
            .values()
            .take_while(|l| self.is_better(l.price, price) || l.price.raw == price.raw)
            .map(|l| l.total_remaining())
            .sum()
    }
}

/// Full instrument book (both sides).
///
/// One `SideBook` per side, fully independent — insertion never rejects or
/// crosses (matching decisions belong to the engine); cross detection is
/// an explicit read used by parity checks and market-status logic.
///
/// # Examples
///
/// ```
/// use exchange8_book::Book;
/// use exchange8_contracts::Side;
///
/// let b = Book::new();
/// assert!(!b.is_crossed());
/// assert_eq!(b.side(Side::Bid).side(), Side::Bid);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    /// Buy side, best (highest) price first.
    pub bids: SideBook,
    /// Sell side, best (lowest) price first.
    pub asks: SideBook,
}

impl Book {
    /// Create an empty two-sided book.
    pub fn new() -> Self {
        Book {
            bids: SideBook::new(Side::Bid),
            asks: SideBook::new(Side::Ask),
        }
    }

    /// Read-only access to one side (mirrors and depth readers).
    pub fn side(&self, side: Side) -> &SideBook {
        match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        }
    }

    /// Mutating access to one side (single-writer matching path).
    pub fn side_mut(&mut self, side: Side) -> &mut SideBook {
        match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        }
    }

    /// Crossed when best bid ≥ best ask; `false` when either side is
    /// empty.
    ///
    /// The engine (and parity tests) use this to stand behind post-only
    /// kills and quotation-only behavior; an empty side is never treated
    /// as crossed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use exchange8_book::{Book, RestingOrder};
    /// # use exchange8_contracts::{Side, Visibility};
    /// # use exchange8_fixed::{Price, Qty};
    /// # fn order(side: Side, px: i128, qty: u64) -> RestingOrder {
    /// #     RestingOrder {
    /// #         order_id: 1, sub_order_id: 0, customer_id: 0,
    /// #         connection_id: 0, side,
    /// #         price: Price::equity(px), full_volume: Qty::units(qty),
    /// #         remaining: Qty::units(qty), visible: Qty::units(qty),
    /// #         max_floor: Qty::units(0), iceberg_sub_order_count: 0,
    /// #         is_odd_lot: false, anonymous: false, stp_key: None,
    /// #         seq: 0,
    /// #     }
    /// # }
    /// let mut b = Book::new();
    /// b.bids.insert(order(Side::Bid, 10_200, 100));
    /// b.asks.insert(order(Side::Ask, 10_100, 100));
    /// assert!(b.is_crossed());
    /// ```
    pub fn is_crossed(&self) -> bool {
        match (self.bids.best(), self.asks.best()) {
            (Some(b), Some(a)) => b.price.raw >= a.price.raw,
            _ => false,
        }
    }
}

impl Default for Book {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exchange8_contracts::Visibility;
    use exchange8_fixed::{Price, Qty};

    fn order(id: u64, price_raw: i128, qty: u64, _vis: Visibility) -> RestingOrder {
        RestingOrder {
            order_id: id,
            sub_order_id: 1,
            customer_id: 1,
            connection_id: 1,
            side: Side::Bid,
            price: Price::equity(price_raw),
            full_volume: Qty::units(qty),
            remaining: Qty::units(qty),
            visible: Qty::units(qty),
            max_floor: Qty::units(0),
            iceberg_sub_order_count: 0,
            is_odd_lot: false,
            anonymous: false,
            stp_key: None,
            seq: 0,
        }
    }

    #[test]
    fn bids_sort_descending_asks_ascending() {
        let mut b = Book::new();
        b.bids.insert(order(1, 10_000, 100, Visibility::Display));
        b.bids.insert(order(2, 10_100, 100, Visibility::Display));
        b.asks.insert(order(3, 10_200, 100, Visibility::Display));
        b.asks.insert(order(4, 10_150, 100, Visibility::Display));
        assert_eq!(b.bids.best().unwrap().price.raw, 10_100);
        assert_eq!(b.asks.best().unwrap().price.raw, 10_150);
    }

    #[test]
    fn fifo_within_level() {
        let mut sb = SideBook::new(Side::Bid);
        sb.insert(order(1, 10_000, 100, Visibility::Display));
        sb.insert(order(2, 10_000, 100, Visibility::Display));
        let level = sb.best().unwrap();
        assert_eq!(level.orders[0].order_id, 1);
        assert_eq!(level.orders[1].order_id, 2);
        assert!(level.orders[0].seq < level.orders[1].seq);
    }

    #[test]
    fn visible_hidden_separate_levels() {
        // Same price, different visibility → 2 levels, visible first.
        let mut sb = SideBook::new(Side::Bid);
        sb.insert(order(1, 10_000, 100, Visibility::Display));
        let mut hidden = order(2, 10_000, 100, Visibility::Hidden);
        hidden.visible = Qty::units(0);
        sb.insert(hidden);
        let levels = sb.levels();
        assert_eq!(levels.len(), 2);
        assert!(levels[0].visible);
        assert!(!levels[1].visible);
        assert!(sb.best_visible().unwrap().visible);
    }

    #[test]
    fn best_visible_skips_hidden_only_book() {
        let mut sb = SideBook::new(Side::Ask);
        let mut hidden = order(1, 10_000, 100, Visibility::Hidden);
        hidden.visible = Qty::units(0);
        sb.insert(hidden);
        assert!(sb.best_visible().is_none());
        assert!(sb.best().is_some());
    }

    #[test]
    fn hidden_better_price_precedes_visible_worse() {
        // Price dominates the visibility tiebreak: a hidden level at a
        // better price sorts before a visible one at a worse price.
        let mut sb = SideBook::new(Side::Ask);
        sb.insert(order(1, 10_100, 100, Visibility::Display)); // visible, worse
        let mut hidden = order(2, 10_000, 100, Visibility::Hidden); // hidden, better
        hidden.visible = Qty::units(0);
        sb.insert(hidden);
        assert_eq!(sb.best().unwrap().price.raw, 10_000);
        assert!(!sb.best().unwrap().visible);
        assert_eq!(sb.best_visible().unwrap().price.raw, 10_100);
    }

    #[test]
    fn crossed_market_detection() {
        let mut b = Book::new();
        b.bids.insert(order(1, 10_200, 100, Visibility::Display));
        b.asks.insert(order(2, 10_100, 100, Visibility::Display));
        assert!(b.is_crossed());
        let mut b2 = Book::new();
        b2.bids.insert(order(1, 10_000, 100, Visibility::Display));
        b2.asks.insert(order(2, 10_100, 100, Visibility::Display));
        assert!(!b2.is_crossed());
    }

    #[test]
    fn volume_at_or_better_fok_check() {
        let mut sb = SideBook::new(Side::Ask);
        sb.insert(order(1, 10_000, 100, Visibility::Display));
        sb.insert(order(2, 10_100, 200, Visibility::Display));
        sb.insert(order(3, 10_200, 400, Visibility::Display));
        assert_eq!(sb.volume_at_or_better(Price::equity(10_100)), 300);
        assert_eq!(sb.volume_at_or_better(Price::equity(10_300)), 700);
        assert_eq!(sb.volume_at_or_better(Price::equity(9_000)), 0);
    }

    #[test]
    fn volume_at_or_better_bids_inverted() {
        let mut sb = SideBook::new(Side::Bid);
        sb.insert(order(1, 10_200, 400, Visibility::Display));
        sb.insert(order(2, 10_100, 200, Visibility::Display));
        sb.insert(order(3, 10_000, 100, Visibility::Display));
        assert_eq!(sb.volume_at_or_better(Price::equity(10_100)), 600);
        assert_eq!(sb.volume_at_or_better(Price::equity(9_900)), 700);
    }

    #[test]
    fn remove_and_prune() {
        let mut sb = SideBook::new(Side::Bid);
        sb.insert(order(1, 10_000, 100, Visibility::Display));
        sb.insert(order(2, 10_100, 100, Visibility::Display));
        let removed = sb.remove(2).unwrap();
        assert_eq!(removed.order_id, 2);
        sb.prune();
        assert_eq!(sb.levels().len(), 1);
        assert_eq!(sb.best().unwrap().price.raw, 10_000);
        assert!(sb.remove(999).is_none());
    }

    #[test]
    fn remove_self_prunes_level() {
        let mut sb = SideBook::new(Side::Bid);
        sb.insert(order(1, 10_000, 100, Visibility::Display));
        sb.insert(order(2, 10_000, 100, Visibility::Display));
        sb.remove(1);
        sb.remove(2);
        assert!(sb.levels().is_empty());
        assert!(sb.best().is_none());
    }

    #[test]
    fn iceberg_helpers() {
        let mut o = order(1, 10_000, 1000, Visibility::Display);
        assert!(!o.is_iceberg());
        o.max_floor = Qty::units(100);
        o.visible = Qty::units(100);
        assert!(o.is_iceberg());
        assert!(o.has_hidden_part());
    }

    #[test]
    fn mixed_sides_independent_best() {
        // The key rank is side-relative; a regression would cross-wire
        // best-price math between the two sides.
        let mut b = Book::new();
        b.bids.insert(order(1, 10_000, 100, Visibility::Display));
        b.asks.insert(order(2, 10_000, 100, Visibility::Display));
        assert_eq!(b.bids.best().unwrap().price.raw, 10_000);
        assert_eq!(b.asks.best().unwrap().price.raw, 10_000);
        // Equal best prices cross by definition.
        assert!(b.is_crossed());
    }

    // ------------------------------------------------------------------
    // Complexity invariants (hot-path contract)
    // ------------------------------------------------------------------

    #[test]
    fn large_book_operations_scale() {
        // 10k levels × 10 orders: a linear-scan level search is quadratic
        // (~seconds); the ordered-map implementation stays in milliseconds.
        let now = std::time::Instant::now();
        let mut sb = SideBook::new(Side::Ask);
        let mut id = 0u64;
        for level in 0..10_000i128 {
            for _ in 0..10 {
                id += 1;
                sb.insert(order(id, 10_000 + level, 100, Visibility::Display));
            }
        }
        assert_eq!(sb.levels().len(), 10_000);
        assert_eq!(sb.best().unwrap().price.raw, 10_000);
        // Indexed remove from the FIRST level (order id 5).
        sb.remove(5);
        assert_eq!(sb.best().unwrap().orders.len(), 9);
        // FOK aggregation across all levels: 10k levels × 10 orders × 100
        // units, minus the one removed order (100).
        assert_eq!(
            sb.volume_at_or_better(Price::equity(19_999)),
            10_000 * 10 * 100 - 100
        );
        let elapsed = now.elapsed();
        // Guard: well under 5s (a linear implementation exceeds 60s).
        assert!(elapsed.as_secs() < 5, "book ops too slow: {elapsed:?}");
    }

    #[test]
    fn insert_preserves_best_first_iteration() {
        // Interleaved insert order must not affect iteration order.
        let mut sb = SideBook::new(Side::Ask);
        sb.insert(order(1, 10_200, 100, Visibility::Display));
        sb.insert(order(2, 10_000, 100, Visibility::Display));
        sb.insert(order(3, 10_100, 100, Visibility::Display));
        let prices: Vec<i128> = sb.iter_levels().map(|l| l.price.raw).collect();
        assert_eq!(prices, vec![10_000, 10_100, 10_200]);
    }

    // ------------------------------------------------------------------
    // Matching discipline: default FIFO within levels (per-contract
    // allocation policies live in `alloc`)
    // ------------------------------------------------------------------

    #[test]
    fn deep_level_fills_strictly_by_arrival() {
        // 50 orders at one price consumed front-to-back: the fill sequence
        // MUST be 1..=50 by arrival. An accidental pro-rata or split-fill
        // allocation breaks this invariant immediately.
        let mut sb = SideBook::new(Side::Ask);
        for id in 1..=50u64 {
            sb.insert(order(id, 10_000, 100, Visibility::Display));
        }
        let mut fill_order = Vec::new();
        while let Some(level) = sb.best() {
            if level.is_empty() {
                break;
            }
            let front = &level.orders[0];
            fill_order.push(front.order_id);
            sb.remove(front.order_id);
        }
        assert_eq!(fill_order, (1..=50u64).collect::<Vec<u64>>());
    }
}
