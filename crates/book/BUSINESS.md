# BUSINESS.md: exchange8::book — Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::book library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

The order-book library is where the exchange's core fairness promise lives:
it holds every resting order, decides how incoming demand consumes that
liquidity, and makes those decisions exactly reproducible. Every tradable
event the exchange publishes rests on this library behaving identically
under a replay — market participants can trust that fills are awarded by
the same rule set, in the same order, every time. In business terms,
`exchange8::book` is the machinery that turns "the exchange is fair and its
depth is truthful" from a promise into data-structure behavior.

- Guarantees fair, priority-correct consumption of resting liquidity.
- Guarantees depth accuracy: displayed quantity is never inflated by
  hidden liquidity.
- Guarantees deterministic, auditable fill allocation per contract.

## 2. Business Scope (What the Service Provides)

The library is not a service; its capabilities are provided to the
matching engine and, through it, to every exchange participant.

| Capability | Description | Consumed by |
|---|---|---|
| Two-sided book keeping | Holds all resting bids and asks per instrument, ordered best-price first, arrival-correct within each price | Ex8:ME |
| Display-depth separation | Visible and hidden orders form distinct levels; displayed depth reflects displayable liquidity only | Ex8:ME, Ex8:MD mirrors |
| Best-price queries | Instant answers to "what is the best (displayable) bid/ask" | Ex8:ME (post-only, crossing), mirrors |
| Depth volume queries | Cumulative volume up to a price limit — the fill-or-kill feasibility question | Ex8:ME (FOK/IOC decisions) |
| Order lifecycle mutations | Resting, in-place partial fills, cancels, full consumption, priority re-entry | Ex8:ME |
| Per-contract allocation | FIFO (equities), round-lot pro-rata (commodity futures), pro-rata with FIFO remainder (hybrid products) — all deterministic integer-share decisions | Ex8:ME (per contract master) |
| Iceberg slice lifecycle | Displayed-quantity reload and time-priority reset bookkeeping per iceberg slice | Ex8:ME |
| Serializability | Every book state can be serialized for mirrors/replay reconstruction | journal recovery, mirrors |
| Simulation parity | Deterministic behavior lets simulation and production share one book implementation | sim-harness, E2E suites |

**What the library does NOT provide (business boundary):**

| Concern | Owning unit |
|---|---|
| Deciding *when* an order matches (post-only, price protection, STP, TIF policies) | Ex8:ME |
| Trading halts, quotation-only status, auction mechanics | Ex8:ME + Ex8:RefData |
| Trade ids, execution envelopes, drop-copy and audit events | Ex8:ME / Ex8:Clearing / platform audit plane |
| Contract metadata (which allocation method applies per instrument) | Ex8:RefData contract master |
| Order admission (risk limits, self-trade policy application) | Ex8:Risk / Ex8:OMS / Ex8:ME |
| Market-data publishing and feed distribution | Ex8:MD |
| Persistence, journaling, and durability | Ex8:ME + platform storage |

## 3. Business Capabilities

**Book integrity**

| Capability | Business rule |
|---|---|
| Priority-correct depth | The book always reads best-price-first; a market-data reader or a matching decision sees levels in execution-priority order, guaranteed structurally, not by convention. |
| Hidden liquidity never displayed | Orders resting anonymously or as iceberg reserve never count toward displayed depth; a hidden level at a better price still wins matching (price dominates), but bypass-style flows see only displayable liquidity. |
| Accurate cancels | A cancellation removes exactly the named order in constant time regardless of book depth; unknown ids are surfaced as not-found, never silently dropped. |
| Exact fill arithmetic | Reducing an order by a fill can never make either its remaining or displayed quantity negative; depth arithmetic saturates rather than corrupts. |
| Lossless snapshot & reconstruction | Any book state can be serialized and later reconstructed; mirror and replay consumers share the reconstruction contract. |

**FIFO fairness**

| Capability | Business rule |
|---|---|
| Arrival order is exchange-assigned | The book itself assigns time priority in arrival order; participants cannot influence their queue position by submitting forged sequence values. |
| Strict prefix consumption (equities) | On a FIFO contract, an incoming order that partially fills the level completes the oldest orders fully, partially fills exactly the next-oldest, and leaves all later orders untouched. |
| Priority reset is explicit | Only documented events (iceberg reload, size/price-modifying replace) reset an order's time priority, and each such event is marked with an incremented slice counter so downstream attribution is unambiguous. |
| Odd-lot separation | Odd-lot portions of mixed orders live outside the board book and never take liquidity from board orders, preserving the odd-lot protection regime for retail-sized participants. |

**Allocation accuracy**

| Capability | Business rule |
|---|---|
| Conservation | Across every allocation policy, the total filled quantity equals exactly the lesser of the incoming quantity and the level's available quantity — participants are never over-filled or shorted by rounding. |
| Per-order caps | No resting order is ever filled more than its resting quantity; pro-rata grants respect each participant's displayed commitment. |
| Deterministic remainder | When a pro-rata share does not divide evenly, the indivisible remainder is awarded by a published rule — largest fractional remainder, ties broken by arrival — never by chance. |
| Round-lot convention | Pro-rata allocation operates on whole integer shares, mirroring round-lot market convention for commodity futures; hybrid contracts receive pro-rata core with FIFO remainder. |
| Reproducibility | For an identical input sequence, the allocation history is identical byte-for-byte on any host, which makes regulator and member inquiries answerable by exact replay. |

## 4. Business Events (Outbound)

A library emits no events; it has no transport. The business events the
exchange's participants observe are produced by the consuming services —
each book operation below enables its corresponding observable effect:

| Book operation | Observable business effect (emitted by the consuming engine) |
|---|---|
| insert | Order accepted and resting ("booked"), appearing in depth |
| reduce / remove on match | Trade events for both sides, depth updates, iceberg slice reload notifications |
| remove by cancel | Cancel confirmation, depth shrink |
| remove on authority halt | Mass cancel on halt |
| volume_at_or_better | Fill-or-kill feasibility decisions visible as fills vs kills |
| is_crossed | Crossing checks standing behind post-only and quotation-only behaviors |

## 5. Business Rules & Invariants

1. Best-price-first depth is structural: displayed depth is always ordered
   from the best price, with visible levels ahead of hidden at the same
   price.
2. Price dominates visibility for matching: a hidden order at a better
   price is matched before a visible order at a worse price.
3. Within any single price level, fills follow arrival order; arrival
   sequence is minted by the book itself.
4. On FIFO contracts, consumption is a strict prefix: no later order fills
   while an earlier order at the same price remains unfilled beyond the
   partial boundary.
5. The fill total of any allocation equals min(incoming quantity, level
   available quantity); each individual fill never exceeds its order's
   resting quantity.
6. Pro-rata remainder awards are deterministic and published (largest
   remainder, arrival tie-break) or FIFO per hybrid contract; the same
   inputs always produce the same fills.
7. An iceberg's displayed quantity never exceeds its declared maximum
   display size; a depleted display reloads from reserve and the reloaded
   slice re-enters at the tail (time priority resets).
8. Hidden liquidity never appears in displayed depth, and bypass-style
   flows never touch hidden levels.
9. Odd-lot liquidity never trades against board-lot liquidity.
10. Quantity arithmetic can never produce negative depth; fills saturate.
11. Identical accepted input sequences produce identical fill sequences and
    identical final book states — the foundation of replay-based dispute
    resolution.

## 6. Target-User Roles & Use Cases

| Role | How the library serves them |
|---|---|
| Matching-engine engineers | Build the exchange's matching policy on top of priority-correct containers and per-contract allocation, without re-implementing book mechanics per product. |
| Market-data product managers | Rely on structurally separated visible depth so every published depth product (top-of-book, market-by-price, depth snapshots) is consistent with what participants can trade/display. |
| Equities product owners | Get the fair, time-prioritized market their participants expect, with FIFO prefix discipline guaranteed by tests, not by convention. |
| Commodity-futures product owners | Get round-lot pro-rata allocation with published remainder rules, per the contract master, using the same engine plumbing. |
| Hybrid-product product owners | Get pro-rata core fills with FIFO remainder, selected per contract. |
| Iceberg/anonymous-order users | Get display integrity: only committed display quantities are visible, reserve never leaks. |
| Compliance & surveillance analysts | Rely on deterministic replay: any historical fill list can be regenerated exactly for inquiries. |
| QA & simulation users | Reuse the same book implementation in simulation so test results transfer to production behavior. |

## 7. KPIs / Success Criteria

| KPI | Target / meaning |
|---|---|
| Book operation latency | Best/best-level, indexed cancel/reduce, and front consumption run effectively in constant time relative to resting order count; a scale guard test holds the contract against a large book. |
| Allocation conservation | 100% of allocation runs conserve quantity exactly (fill sum = min(incoming, level volume)) — property-tested across every policy, level shape, and incoming size. |
| Oversell rate | Zero: no fill exceeds its resting quantity in any policy run. |
| FIFO discipline violations | Zero: strict-prefix and deep-level arrival tests must remain permanently green. |
| Depth accuracy | Zero incidents of hidden liquidity appearing in displayed depth in parity tests against golden event streams. |
| Reproducibility | Identical replays produce identical fills byte-for-byte; zero nondeterminism findings. |
| Library stability | Zero `unsafe` code; no panic paths reachable from user input. |

## 8. Dependencies & Reliance on Other Services

| Relies on | For |
|---|---|
| `exchange8::fixed` | Price/quantity value types and their scale conventions |
| `exchange8::contracts` | Order-side and visibility domain enums |
| Ex8:RefData contract master | Which allocation method (and lot structure) applies to each instrument |
| Ex8:ME | The single-writer execution of book operations, matching decisions, iceberg reloads, and STP application |
| Platform sequence plane | Trade-id sequencing and audit attribution at the engine boundary |

The library itself depends on nothing external beyond those value types and
standard-library containers — no network, no database, no platform service,
which is what makes its guarantees testable in isolation.

## 9. Acceptance Criteria

1. A book consumer can rest, cancel, partially fill, and fully consume
   orders with the documented constant-time/`O(log L)` behavior and always
   read depth in best-first order.
2. All three allocation methods produce conserving, capped, deterministic
   fill lists, with the FIFO default certified as a strict prefix by test.
3. Visible and hidden orders at the same price land in separate levels,
   visible first, with hidden liquidity excluded from displayed depth
   queries.
4. Iceberg display reload and time-priority reset behave exactly as the
   matching-engine design prescribes, with slice counts incrementing per
   reload.
5. The allocation-method selector round-trips the contract-master values
   exactly, and any unknown value is rejected rather than defaulted.
6. Every public item of the library is documented with usage examples, and
   the crate builds warning-free.
7. The large-book complexity guard passes within its time budget,
   demonstrating the sub-linear hot-path contract.

## 10. Open Business Questions

- **Pooled order storage.** If steady-state fill-path allocation costs
  become a measurable capacity constraint, pooled (arena-backed) order
  storage would trade some of the library's serialization simplicity for
  allocation reuse — a product decision between engine throughput and
  mirror simplicity.
- **Depth view ownership.** Whether the library should publish an
  aggregated depth view (per price rung, book/odd partitioned) as a first
  class capability instead of leaving the fold to each consumer — reduces
  duplication, but adds a derived surface whose divergence from raw levels
  must be governed.
- **Additional allocation policies.** Size-priority or other regional
  conventions would extend the per-contract policy set; each addition needs
  a published remainder rule to preserve the determinism guarantee.
- **Cross-side accessor scope.** Whether post-only/crossing logic should
  consume a central two-sided primitive from the library rather than
  engine-side derivations, as those consumer paths evolve.

---

**Design attribution:** exchange8:book and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
