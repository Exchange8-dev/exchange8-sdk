# exchange8::fixed — Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::fixed library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

- Guarantee that every price and quantity the exchange computes is exact:
  no floating-point drift, no silent rounding, no loss of a single tick.
- Keep asset-class precision conventions equitable and crypto honest:
  equities price to 4 decimals, FX to 5 (pip-level), crypto to 8 — in one
  shared definition rather than per-service folklore.
- Protect platform solvency math: notional values (price × quantity) are
  computed with overflow checks so an impossible order can never produce
  a wrapped, wrong money number.
- Make board-lot marketplace rules computable: whole-lot versus odd-lot
  volumes, minimum tick grids per price band, and doubled-tick validity
  for midpoint orders.

## 2. Business Scope (What the Library Provides)

| Capability | What it delivers (business terms) |
|---|---|
| Price representation | Scaled integer prices that carry their decimal precision, per asset class |
| Quantity representation | Integer board-lot quantities and fractional (crypto) quantities |
| Decimal parsing | Turning "10.05", "1.08456", "0.5" into exact scaled integers, rejecting malformed or out-of-range input |
| Scale conversion | Re-expressing a value at a different precision, only when lossless |
| Checked arithmetic | Scale-matched addition, tick stepping, and quantity subtraction that report overflow instead of wrapping |
| Notional computation | Price × quantity with multiply-first precision, in price tick units |
| Board/odd-lot split | Whole-lot count plus odd-lot remainder for a quantity at a given lot size |
| Tick validation | Equity band ticks (1¢ / 0.5¢), doubled-tick grid for midpoint prices, FX whole-pip and half-pip grids |
| Conformance tests | In-crate tests pinning parse rules, tick bands, FX pip grids, rescale losslessness, and notional overflow |

### What the service does NOT own (business boundary)

| Concern | Owned by |
|---|---|
| Matching decisions and book state | Exchange8:ME |
| Order lifecycle and peg rules | Exchange8:OMS |
| Limit / exposure policy over values | Exchange8:Risk |
| Venue quote interpretation | Exchange8:VFeeds / Exchange8:MD adapter layers |
| FIX order-entry string formatting | `exchange8::fix`, Exchange8:Gateway |
| Display formatting and localization | Consumer applications |
| Persistence, identity, audit, event delivery | Sentin8:DB, Sentin8:Auth, Sentin8:Sys, Sentin8:Bus platform planes |

## 3. Business Capabilities

| Area | Capability | Business rule |
|---|---|---|
| Precision | Asset-class scales | Equities trade on 4-decimal prices, FX on 5-decimal pip prices, crypto on 8-decimal prices; quantities are whole board lots except crypto, which tolerates 8-decimal fractions |
| Integrity | Scale discipline | Values at different precisions never mix without an explicit, lossless conversion |
| Integrity | positivity | An exchange price is always strictly positive; zero and negative prices do not parse |
| Safety | Overflow reporting | Any money arithmetic that would exceed platform magnitude is reported, never silently wrapped |
| Marketplace | Tick grids | Sub-half-dollar equity prices trade in half-cent increments; at or above, in whole cents; midpoint/peg prices validate on the doubled grid |
| Marketplace | FX pip rules | Quotes are whole-pip (0.0001) by default; venues streaming fifth-digit prices validate on half-pip increments |
| Marketplace | Board lots | A quantity splits into a whole-lot count plus an odd-lot remainder; fractional quantities are treated entirely as odd-lot volume |
| Notional | Money value | Notional is price × quantity computed exactly, expressed in price tick units |

## 4. Business Events (Outbound)

- None. The library is a pure function library: it emits no events, writes
  no journal, and communicates nothing. The events of the marketplace —
  executions, order updates, limit alerts — belong to the services that
  use these values (Exchange8:ME, Exchange8:OMS, Exchange8:Risk, and their peers).

### Inbound (consumed)

- None.

## 5. Business Rules & Invariants

1. Every price and quantity in the exchange8 vertical is a scaled integer
   carrying its decimal precision; raw integers without scale never exist.
2. Equities: 4 decimals; FX: 5; crypto: 8 — identical across every
   consuming service.
3. Prices are strictly positive; non-positive decimal input is rejected.
4. Two values of different precision never combine implicitly; conversion
   between precisions must be exact or it is refused.
5. Arithmetic overflow is reported, not wrapped.
6. A notional result is expressed in the price's tick units and computed
   at full precision (multiply before divide).
7. Tick-grid validation is by price band: half-cent increments below
   $0.50, whole cents at or above; midpoint prices validate on the
   doubled grid.
8. Board-lot decomposition is exact: lots × lot size + odd = quantity.

## 6. Target-User Roles & Use Cases

| Persona | What they do with it |
|---|---|
| Matching-engine engineer | Prices and quantities in the hot book path; FX-scale conversions |
| OMS engineer | Peg-price validation against the doubled equity tick grid |
| Risk engineer | Limit checks over checked quantities and notionals |
| Clearing/settlement engineer | Notional computation and lot decomposition for obligations |
| Market-data engineer | BBO and price-band logic on venue-derived values |
| QA / replay engineer | Verifies identical numeric results across replay and live paths |

## 7. KPIs / Success Criteria

| Measure | Target |
|---|---|
| Numeric determinism | Identical arithmetic results across services, hosts, and replay |
| Overflow safety | Zero silent wraps; every checked operation reports on overflow |
| Precision fidelity | Zero cross-scale mixing incidents; lossless conversions guaranteed by tests |
| Cost | Pure in-process value operations; negligible against service logic |
| Dependency footprint | No runtime dependency beyond serde for boundary serialization |

## 8. Dependencies & Reliance on Other Services

- **Owning consumers (Exchange8:ME, Exchange8:OMS, Exchange8:Risk, Exchange8:Clearing,
  Exchange8:Settlement, Exchange8:SOR, Exchange8:MD, Exchange8:VFeeds)** — define the business
  rules that consume values; the library defines only the number system.
- **`serde`** — boundary serialization of value pairs across service
  contracts.
- No reliance on Sentin8 platform planes — no identity, authorization,
  persistence, or delivery work is performed here.

## 9. Acceptance Criteria

1. Every market number in the platform deserializes into a scale-carrying
   value and re-serializes unchanged.
2. Decimal strings parse exactly to their scaled representation at the
   configured asset-class scale; over-precision input is rejected.
3. Equity tick rules, the doubled grid, and FX pip/half-pip grids match
   the platform conventions, pinned by in-crate tests.
4. Mixed-precision values never combine implicitly; rescale to a common
   scale is explicit and losslessness-checked.
5. Notional arithmetic on maximal realistic crypto operands completes
   correctly while an intentionally overflowing operand is refused.
6. Board/odd-lot decomposition reproduces the original quantity exactly
   for integer, fractional, and lot-boundary cases.

## 10. Open Business Questions

- **Quantity precision per instrument** is a global convention (integer
  lots, 8-decimal crypto); whether instrument-level metadata should carry
  per-instrument quantity precision is a refdata decision.
- **Additional asset classes** (futures ticks by point value, options
  multipliers) will need further rule functions; the value model itself
  is not expected to change.
- **Formatting/presentation** of scaled values as decimal strings is
  currently a consumer responsibility; a shared rendering helper may be
  worth adding.

---

**Design attribution:** exchange8:fixed and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
