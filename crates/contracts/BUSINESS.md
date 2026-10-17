# exchange8::contracts — Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::contracts library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

The exchange8::contracts library is the shared **domain language** of the
capital-markets platform: every order that enters the exchange, every rejection
a client or broker sees, every execution event that flows to clearing, market
data, and surveillance is described by the business concepts this library
encodes. Its business value is delivered indirectly but completely — by making
the order/reject/event vocabulary **uniform** across order ingestion, risk
authorization, matching, clearing, and drop-copy reporting, the library removes
the misinterpretation class of operational incidents (a rejection reason that
means one thing to the trading gateway and another to clearing) and guarantees
that FIX-connected clients receive rejection and status codes with the exact
numeric values and texts they already understand.

## 2. Business Scope (What the Service Provides)

| Capability | What the business gets |
|---|---|
| Normalized order vocabulary | One order representation used by ingestion, risk, and matching, covering equity, fixed-income, crypto, FX, futures, perpetuals, and options instrument kinds |
| Instrument reference vocabulary | Symbol/ISIN/MIC/currency identity plus per-instrument price and quantity precision and board-lot size, enabling multi-precision trading (equities at 4 decimals, crypto at 8) on one platform |
| FIX-compatible order attributes | Standard side, time-in-force, order-type, and pegging codes familiar to FIX 4.2 order management systems |
| Self-trade-prevention vocabulary | Explicit prevention actions (kill incoming, kill resting, kill both, decrement) and four comparison key types for regulatory self-trade controls |
| Rejection catalog | A single, stable set of rejection reasons with numeric codes and English FIX 103 texts that clients can rely on unchanged |
| Execution-event vocabulary | Standard shapes for order booked/killed/modified/repriced/accepted/filled events and per-side execution events with liquidity, wash-trade, and principal flags |
| Quote vocabulary | Best bid/offer snapshots for both own-book and consolidated multi-venue views |
| Risk authorization contract | The request/response vocabulary for the pre-trade risk check every order must pass |
| Event envelope standard | Every emitted business event carries tenant, sequence, and idempotency identity so downstream processing is exactly-once |

**What the service does NOT own (business boundary):**

| Concern | Owned by |
|---|---|
| Enforcing pre-trade risk limits | Risk service (uses the authorization contract defined here) |
| Matching orders and generating executions | Matching-engine service |
| Presenting FIX sessions to clients | Gateway service |
| Deciding which rejection applies to which failure | Gateway/Risk/OMS business logic |
| Persisting orders and trades | Clearing/OMS services |
| Transport framing and delivery of events | Wire-transport crate and the Sentin8 bus |
| Client-facing market screens | Market-data and venue-feed services |

## 3. Business Capabilities

### Order vocabulary

| Capability | Business rule behind it |
|---|---|
| Order identity | Every order carries a gateway-assigned identifier, the client's order reference, the submitting session, and the tenant environment — enabling client inquiries, audit, and tenant separation |
| Order sides | Six sides (buy, sell, short sell, short sell exempt, cross, cross short) covering regulated short-sale and crossing flows |
| Time in force | Day, good-till-cancel, at-open, immediate-or-cancel, fill-or-kill, crossing, good-till-date; an omitted time-in-force defaults to Day |
| Order pricing | Limit and pegged order types only; pegging styles (midpoint, market, primary, darkpool midpoint) with an explicit price difference |
| Execution visibility | Displayed or hidden quantity, with optional floor size for iceberg display |
| Capacity and anonymity | Agency vs. principal capacity and anonymous execution flags, preserving regulatory reporting categories |
| Self-trade prevention | Each order may carry a prevention key (customer, location, connection, account) and an action (kill incoming, kill resting, kill both, decrement) — explicit and auditable per order |
| Price protection | Orders may request kill-on-violation or post-after-sidestep protection against crossing a marketable price band |
| Order books | Orders declare their lit/hidden book token and optional strategy-book list |

### Rejection vocabulary

| Capability | Business rule behind it |
|---|---|
| Client-visible rejection text | Every rejection reason maps to its FIX 103 English text; text is stable per code so client systems can parse it |
| Numeric compatibility | Rejections inherited from the matching-engine protocol keep their exact numeric codes (1–8, 100–144 families) on client-visible rejection reports |
| Platform-only rejections | Risk-unavailability (fail-closed), rate limiting, Greeks limits, insufficient margin, malformed messages, and internal errors are rejection reasons without numeric code — they are never presented as inherited numeric codes |
| Catch-all | Any unrecognized legacy reason collapses to "Unknown reason" rather than a misleading specific code |

### Event vocabulary

| Capability | Business rule behind it |
|---|---|
| Lifecycle events | Booking, killing (with reason), modification, repricing, acceptance, and fill events mirror the matching engine's protocol messages one-for-one |
| Execution events | Each execution is one event per side with a shared trade identifier, liquidity role (added/removed), and execution flags for principal, wash, and midpoint-charge classification |
| Quote events | Best bid/offer snapshots carry venue attribution so consolidated views can be attributed per market |
| Event identity | Every event carries tenant environment, emitter sequence, and an idempotency key — downstream businesses never double-count a delivered event |

### Risk authorization

| Capability | Business rule behind it |
|---|---|
| Pre-trade decision contract | Every submitted order is expressed as the shared authorization request; the decision response is allow or reject with an optional rejection code from the shared catalog |
| Fail-closed posture | When risk is unavailable, the vocabulary's explicit "risk service unavailable" reason makes the fail-closed outcome a first-class, auditable business outcome |

## 4. Business Events (Outbound)

The library defines the event shapes and names consumed by the platform's bus
and direct streams; the producing services own when they fire. The encoded
outbound vocabulary (naming
`exchange8:{service}:{resource}:{operation}`):

- **Order ingestion** — order submitted (Gateway).
- **Order lifecycle** — booked, killed (with reason), modified, repriced,
  rejected (with operation kind and reason), accepted (ME).
- **Executions** — trade executed per side with shared trade id
  (`exchange8:me:trade:executed`).
- **Market state** — instrument halted/opened, book-level market state
  (ME/MD), best-bid/offer updated (`exchange8:md:bbo:updated`), depth
  updated, trade printed/busted/amended, venue quotes and feed status
  (MD/VFeeds).
- **Routing and clearing** — sor leg submitted/filled/cancelled/rejected;
  clearing order recorded and drop-copy sent.

**Inbound (consumed):** the same envelope shapes reach consumers — ME consumes
order-entry operations; Clearing/MD consume lifecycle and execution events;
Surveillance consumes execution events through the standard trigger
subscription. The library itself consumes nothing (it is passive vocabulary).

## 5. Business Rules & Invariants

1. Every rejection presented to a FIX client carries the inherited numeric
   code whenever one exists, plus the exact FIX 103 text of that code.
2. A rejection reason with no inherited numeric code is never rendered as a
   legacy numeric code to clients.
3. Every order accepted into the exchange has passed the risk authorization
   request/response contract; there is no alternate untyped path.
4. Every business event in the platform carries tenant (`env_id`), a
   monotonic per-emitter sequence, and an idempotency key — enabling
   exactly-once downstream processing.
5. Prices and quantities are scaled integers carrying their own precision;
   equity instruments use 4 decimal digits and crypto 8 — the platform never
   mixes precisions silently within one instrument.
6. Order status terminates only at filled, canceled, rejected, or expired;
   any other status promises further updates.
7. Unknown wire characters or FIX codes are never silently mapped onto valid
   variants — they surface as malformed-message or unknown-reason rejections.
8. Character-coded constants (order type, side, visibility, capacity, kill
   reason, liquidity, lot type, statuses) always round-trip exactly to the
   matching-engine protocol values — a code that enters the exchange leaves
   the exchange unchanged.

## 6. Target-User Roles & Use Cases

| Persona | What they do with the vocabulary |
|---|---|
| Retail/institutional trader | Benefits invisibly: their order enters as the shared normalized order and any rejection comes back with the recognizable FIX rejection code and text |
| FIX/OUCH broker connectivity | Integrates once against the stable rejection-code catalog and order-status set; inherited numeric codes match their existing systems |
| Risk/compliance officer | Relies on the uniform authorization contract and fail-closed rejection reasons being distinguishable and auditable across all order flows |
| Clearing operations | Consumes order lifecycle and execution events with stable identities and dedup keys for settlement and drop-copy reporting |
| Market-operations desk | Uses instrument/market state vocabulary (open, halted, quotation-only; reopen/postpone/shutdown) consistently across screens and feeds |
| Quant/algorithmic desk | Depends on precision-preserving price/quantity semantics across equity and crypto instruments |
| Platform integrator | Builds new services against one vocabulary instead of re-declaring order, reject, and event shapes per service |

## 7. KPIs / Success Criteria

- **Vocabulary consistency** — zero divergence incidents between gateway,
  risk, matching engine, and clearing on rejection codes or order statuses
  (the vocabulary is shared by construction).
- **Client compatibility** — every client-visible rejection renders the
  expected numeric code and FIX 103 text; compatibility regressions in the
  vocabulary tables are caught by the in-crate catalog-anchored tests.
- **Event integrity** — near-zero duplicate or lost downstream events
  attributable to the envelope contract (sequence + idempotency key present on
  every emission).
- **Precision correctness** — no price/quantity precision misinterpretation
  across instruments of different scales.
- **Adoption** — order-path services (gateway, risk, matching, clearing,
  market data) share this vocabulary rather than local duplicates.
- **Latency impact** — the vocabulary adds no runtime work on the matching hot
  path (it is compile-time data; conversion accessors are constant-time).

## 8. Dependencies & Reliance on Other Services

- **`exchange8::fixed` (numeric primitives).** The library relies on the
  fixed-point `Price`/`Qty` types for precision-preserving numerics; it adds
  no arithmetic of its own.
- **Serialization (`serde`/`serde_json`).** All persistence and transport of
  the vocabulary is through standard serde JSON.
- **`exchange8::wire` and the ME client crate.** Downstream framing of
  envelopes for the direct matching-engine streams; the library itself is
  transport-agnostic.
- **Producing services (Gateway, Risk, ME, MD, VFeeds, Clearing, SOR).** They
  populate the envelopes; the library guarantees the shape and naming but not
  delivery.
- **The extracted behavior catalog** (`design/behavior-catalog/`). The
  reference for every character code and rejection number the business
  presents externally; the library's tests anchor to it.

## 9. Acceptance Criteria

1. Every order flowing gateway → risk → matching engine uses the shared
   normalized order type — no unit re-declares order fields for those flows.
2. Every order rejection surfaced to a FIX client carries the inherited
   numeric code (when defined) plus the exact FIX 103 text.
3. Fail-closed risk unavailability and rate limiting are expressible and
   distinguishable rejection reasons.
4. Every emitted business event serializes with tenant, sequence, and
   idempotency identity intact in both bus and framed transports.
5. Every enumerated wire constant round-trips exactly against the extracted
   matching-engine tables (enforced by tests, compiler-checked on addition).
6. Equity (4-digit) and crypto (8-digit) price/quantity precision is
   preserved end-to-end, with mis-scaled values detectable rather than
   silently mixed.
7. Adding a new rejection reason or wire constant requires exactly one
   change in this library, and every exhaustive consumer is forced to handle
   it at compile time.
8. No floating-point value representing money or quantity appears anywhere in
   the order/reject/event vocabulary.

## 10. Open Business Questions

- **Coverage of remaining protocol messages.** Two matching-engine protocol
  messages (all-orders state request; exec-post route posting) are not yet
  represented by shared event shapes, so their business-level consumers
  (bulk inquiries, routed-order postings) currently rely on narrower
  service-local handling; whether they belong in the shared vocabulary is
  pending.
- **Event payload completeness.** Some lifecycle event payloads carry less
  business context than the platform's event architecture prescribes
  (self-contained snapshots), which would force downstream consumers to make
  additional reads; harmonizing payload completeness is a business-visible
  trade-off (event size vs. consumer simplicity) awaiting product input.
- **Reject-text localization.** English-only rejection texts today;
  multilingual client-facing rejection text is a potential business
  requirement for international clients.
- **Consolidated-quote fee/venue attribution.** The quote vocabulary carries
  venue attribution for consolidated views, but the business rules around
  fee-eligible venue attribution are not encoded and may need dedicated
  vocabulary.

---

**Design attribution:** exchange8:contracts and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
