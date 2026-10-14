# BUSINESS.md: exchange8::fix — Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::fix library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

- Provide the machine-to-machine **FIX 4.2 order-entry language** of the
  exchange venue so institutional clients can submit, amend, and cancel
  orders electronically instead of over the phone or through bespoke
  integrations — the industry-standard front-office connectivity every
  buy-side and sell-side desk expects.
- Guarantee that every byte exchanged with an order-entry counterparty is a
  well-formed, integrity-checked FIX frame, so order flows are never
  corrupted by framing defects.
- Carry the venue's **RFQ (Request-For-Quote) FX business** on the same
  connectivity: quote requests, tradeable two-way quotes, client hits, and
  terminal quote states all speak FIX quote messages.
- Give business events (fills, cancels, rejects, replaces) a standard,
  counterparty-readable representation the moment they happen — no
  proprietary reporting format for clients to integrate against.

## 2. Business Scope (What the Service Provides)

| Capability | Description |
|---|---|
| Order-entry message encoding | Writes complete FIX 4.2 frames (NewOrderSingle, cancel, replace, status request, execution reports) ready for the order-entry socket |
| Order-entry frame decoding | Reads counterparty bytes and returns validated FIX messages, distinguishing "not yet complete" from "corrupted" |
| Message-integrity enforcement | Every accepted frame passes BodyLength and CheckSum validation before any business layer sees it |
| Session-message vocabulary | Logon/heartbeat/test-request/resend/reset/logout message types the session engine uses to keep connectivity healthy |
| Execution lifecycle vocabulary | ExecutionReport/CancelReject/BusinessReject message types and all lifecycle tags (status, quantities, prices, reject reasons) |
| FX RFQ quote flow | QuoteRequest / two-way tradeable Quote / quote-accept / QuoteStatusReport constructors with quote identifiers, bid/offer prices, validity window, and value dates |
| Venue-dialect fields | Custom tags for identity/attribution (UMIR cluster), order character (anonymous, program trade, post-only, bypass, protection), book/routing selection, and liquidity flags |
| Numeric-safe value handling | Prices and quantities cross as exact strings — the venue never rounds or re-scales a client's figures |

### What the library does NOT own (business boundary)

| Concern | Owning unit (business view) |
|---|---|
| Session rules (logon procedure, sequence numbers, resends, gaps, disconnects) | Ex8:Gateway order-entry service |
| Credential checking and session authentication | Sentin8:Auth + Ex8:Gateway |
| Order validation (symbol resolution, limits, risk pre-checks) | Ex8:Gateway submission pipeline + Ex8:Risk |
| Matching, executions, and trade lifecycle | Ex8:ME and downstream services |
| Price/qty arithmetic and display precision | exchange8::fixed values library |
| Connectivity for other wire protocols (ITCH feeds, OUCH, stamp) | Their own protocol libraries, consumed by Ex8:Gateway |

## 3. Business Capabilities

### 3.1 Order entry

| Capability | Business rule |
|---|---|
| New order submission | A client's order intent (symbol, side, quantity, limit price, time-in-force, venue-dialect attributes) is carried in NewOrderSingle form with no re-interpretation of figures |
| Order amendments | Cancel and replace requests carry the original client order id so the venue can trace the lifecycle chain |
| Order status | Clients can query order status; lifecycle reports use the standard status vocabulary (new / partial / filled / canceled / replaced / rejected) |
| Rejection transparency | Execution and cancellation rejections carry the venue's reason codes and human-readable text |

### 3.2 FX RFQ business

| Capability | Business rule |
|---|---|
| Quote request | A client requests a price for a currency pair with quantity, side, and tenor (spot, overnight, weeks, months) |
| Tradeable quotes | The venue returns a binding two-way price (bid/offer) valid until an explicit expiry instant, with the settlement value date when applicable |
| Quote execution | A client accepts a quote as-is; the venue confirms acceptance or terminates the RFQ with a status and reason (accepted / expired / rejected) |
| Price fidelity | Quote prices are decimal strings exactly as produced — never re-scaled between engine and counterparty |

### 3.3 Session administration

| Capability | Business rule |
|---|---|
| Keepalive | Heartbeat negotiation at logon, bounded to a 10–120 second interval with a 30-second default, keeps sessions healthy and detects dead connections |
| Loss recovery | Sequence-number gaps are detected and recoverable: the venue's session engine requests resends and re-delivers stored reports marked as possible duplicates |
| Clock discipline | Resend/sequence-reset messages carry the fields standard FIX consumers rely on to reconcile sequence state |

## 4. Business Events (Outbound)

This library emits **no business events** — it is the wire dialect itself.
Every FIX message it encodes is the vehicle for a business event owned
elsewhere:

| FIX message (this library's vocabulary) | Business meaning | Business owner |
|---|---|---|
| 35=D / F / G / H | Client order intent and lifecycle actions | Ex8:Gateway order pipeline |
| 35=8 ExecutionReport | Order accepted / filled / cancelled / rejected / replaced | Ex8:ME → Ex8:Gateway reporting |
| 35=9 / 35=j | Cancel rejected / business rejected | Ex8:Gateway |
| 35=R / s / aj / AJ | FX RFQ request / quote / accept / terminal state | Ex8:RFQ FX business |
| 35=A / 0 / 1 / 2 / 3 / 4 / 5 | Session lifecycle (logon, keepalive, resync, logout) | Ex8:Gateway session engine |

Inbound (consumed): FIX frames arriving from order-entry counterparties;
the library validates framing and hands structured messages to Ex8:Gateway.

## 5. Business Rules & Invariants

1. A counterparty can never receive a frame the venue has not
   integrity-sealed: BodyLength and CheckSum are always recomputed, never
   copied from a cached message.
2. A business layer can never act on a frame that failed integrity
   validation — parsing either succeeds completely or reports a
   structured failure, never a partially valid message.
3. Any client message the venue acknowledges has round-tripped through
   encode/parse identically: figures sent are figures received, byte for
   byte.
4. Truncated network deliveries are never treated as business errors —
   the venue waits for the rest of the frame rather than rejecting a
   half-delivered order.
5. Prices, quantities, and identifiers cross the boundary as exact text
   strings; no precision is lost inside the connectivity layer.
6. Only `FIX.4.2` frames are accepted or produced by this codec; any other
   protocol version requires an explicit session-layer decision elsewhere,
   never a silent fallback.

## 6. Target-User Roles & Use Cases

| Persona | What they do with the library (through Ex8:Gateway) |
|---|---|
| Buy-side institutional desk | Submits electronic orders and receives execution reports in the industry-standard dialect their OMS already speaks |
| Vendor / broker-dealer | Connects a multi-client order-entry session, attributing orders to beneficiaries on the wire |
| FX client | Requests FX quotes and hits tradeable two-way prices within the validity window |
| Liquidity-provider desk (FX) | (Session-side) delivers quotes/last-look outcomes that the platform expresses via this codec's quote vocabulary |
| Venue operations engineer | Relies on deterministic parse/reject behaviour to diagnose counterparty connectivity issues from reject codes alone |
| Quant / integration engineer | Integrates against the venue using a compact, well-documented codec with executable examples in the API docs |

## 7. KPIs / Success Criteria

- **Connectivity trustworthiness**: 100% of frames delivered to
  counterparties pass checksum/length validation; zero malformed frames
  originate from this codec (round-trip tests own proof).
- **Integrity**: no corrupted counterparty frame is ever processed as a
  business message (hard reject taxonomy).
- **Latency overhead**: codec cost is negligible on the order path —
  per-message cost is one buffer build + one linear field scan; no locks,
  no I/O, no async hops.
- **Coverage**: every message type and tag the order-entry, reporting, and
  FX RFQ businesses use has a named, documented constant (no magic
  numbers in business code).
- **Integration quality**: API examples execute as tests, so the
  documented quick-start never drifts from reality.

## 8. Dependencies & Reliance on Other Services

| Relies on | For |
|---|---|
| Ex8:Gateway | Being the sole transport/session host: sockets, logon rules, sequencing, resend storage, credential checks |
| Exchange8:ME (via Gateway) | The lifecycle events that become ExecutionReport messages |
| Ex8:RFQ / FX business | The quote state machine whose business outcomes the quote messages carry |
| `exchange8::fixed` (sibling library) | Scaled price/quantity arithmetic *outside* this codec — the FIX layer deliberately stays in string space |
| `thiserror` (single dependency) | Structured error taxonomy |

## 9. Acceptance Criteria

1. An order submitted by a counterparty, encoded by this codec, matches
   the wire format of the platform's FIX order-entry specification byte
   for byte (framing math pinned by tests).
2. Every frame produced carries a correct BodyLength (including the
   checksum trailer) and a correct CheckSum; re-parsing any produced frame
   succeeds identically.
3. Every inbound frame failing length or checksum validation is reported
   as a specific, actionable error — never silently repaired, never
   passed downstream.
4. Truncated frames are surfaced as "incomplete" without error so the
   order path never rejects an order because of network framing timing.
5. Session, order-entry, lifecycle, and FX quote message types and tags
   are all first-class named constants; business code contains no raw
   magic tag numbers.
6. All public API documentation includes runnable examples that pass as
   part of the build.

## 10. Open Business Questions

- **FIX 5.0 order entry on this codec**: connectivity currently negotiates
  FIX 5.0 sessions upstream while carrying `FIX.4.2` frames; whether
  client-facing sales collateral and onboarding docs should advertise 4.2
  conformance only is a product decision to align with connectivity
  operations.
- **Multi-leg FX swaps**: the quote vocabulary covers single-value-date
  instruments and a swap far-leg date field; full multi-leg quote
  composition for swap business is deferred until that business line
  requires it.
- **Counterparty dialect matrix**: venue-dialect custom tags
  (identity/attribution cluster) are carried today for order character and
  compliance attribution; whether additional market-dialect remaps should
  be surfaced as named constants (rather than handled in the gateway) is
  an ongoing interoperability question.
- **Quote-window reporting**: whether last-look/quote-window diagnostics
  should eventually ride richer quote messages for operational analytics
  is a product decision for the FX business.

---

**Design attribution:** exchange8:fix and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
