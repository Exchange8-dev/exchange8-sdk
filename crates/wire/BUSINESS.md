# BUSINESS.md: exchange8::wire — Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::wire library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

The exchange8 vertical is a capital-markets platform; its order books, risk
gates and routers move every trading decision through a handful of in-process
handshakes. `exchange8::wire` is the small library that makes those handshakes
trustworthy and fast:

- Every internal trading message travels in a **well-defined frame** whose
  corruption, truncation and oversize cases are handled predictably.
- Every service-to-service hand-off runs through a **bounded queue that never
  loses a message silently** and never blocks the trading hot path.
- Every order id, trade id and child-order id is handed out from an
  **in-process block of leased, unique, strictly increasing ids** — so the
  business gets platform-global uniqueness at in-memory speed.

The business value: market operations that can (1) prove every trade and order
id is unique and traceable, (2) bound and observe queue pressure instead of
crashing or stalling under load, and (3) add new internal services without
re-inventing framing, queueing or id allocation.

## 2. Business Scope (What the Service Provides)

| # | Capability | What the business gets |
|---|---|---|
| 1 | Framed message encoding (`encode`) | A standard envelope for internal trading messages — inspectable JSON payloads with a self-describing length prefix. |
| 2 | Framed message decoding (`decode`) | Trustworthy intake: corrupt, truncated or oversize traffic is contained as a typed error, never silent corruption, never a crash. |
| 3 | Frame size containment (`MAX_FRAME`, 4 MiB) | A hostile or broken peer cannot force the platform into unbounded memory on the internal tier. |
| 4 | Partial frame length inspection (`next_frame_len`) | Streamed readers can size the next read before data arrives — the basis of orderly stream handling. |
| 5 | Bounded in-process queues (`channel`) | Controlled hand-off between service components with an explicit, fixed queue depth. |
| 6 | Backpressure with value preservation (`BoundedSender.try_send` / `SendError`) | Under load, the sender learns *immediately* and the message is handed back — no stall on the hot path, no lost work. |
| 7 | In-process id lease blocks (`SequenceLease`) | Business ids (orders, trades, child orders, rfq trades) are issued unique and increasing at in-memory speed, in platform-leased blocks of 10 000. |
| 8 | Lease exhaustion as an explicit, observable condition | The business rule "never issue an id twice" is structural: an exhausted block simply stops, and the owning service decides to refill or fail-stop. |

### What the service does NOT own (business boundary)

| Concern | Owner | Why it is out of scope here |
|---|---|---|
| Id-counter allocation (the source of uniqueness) | Sentin8 platform sequence-lease facility | The vertical does not own the id-counter substrate; the library only holds in-process blocks received from it. |
| Lease refill policy (when and how blocks are re-armed) | Consuming service shells (`sentin8::shell`) | Refill timing is an availability decision of each service (e.g. matching engine fail-stop vs gateway refill-and-continue). |
| Typed trading messages and event envelopes | `exchange8::contracts` | Domain DTOs and event naming live in the shared contracts crate. |
| The ME stream client (connection, envelope composition) | `exchange8::me::client` | Wire provides framing primitives; the client owns connection semantics. |
| External market protocols (FIX, ITCH, MoldUDP64, SoupBinTCP, STAMP) | dedicated codec crates | Distinct wire formats with their own fidelity requirements — deliberately separate. |
| Transport security, sockets and TLS | consuming services / platform `sentin8::shell` | A library cannot own the network. |
| Authorization, tenant isolation, audit | Sentin8:Auth / Shield / Sentin8:Sys and the service shells | The library runs strictly post-authorization, inside an already-authorized process. |

## 3. Business Capabilities

### ID leasing semantics (business guarantees on ids)

| Capability | Business rule behind it |
|---|---|
| Uniqueness of business ids | An in-process block hands out each id at most once; blocks are received disjoint from the platform facility, so no two services — or two instances of one service — can ever mint the same id. |
| Monotonicity of ids | Ids hand out strictly increasing; across restarts they continue from the platform's value and never reset within an environment — trade sequences read as a stable, auditable history. |
| Gap tolerance | Gaps may appear between blocks (e.g. an abandoned block); the business accepts gaps because duplicates are impossible — uniqueness outranks density. |
| Throughput economics | One platform interaction leases 10 000 ids; id issuance on the hot path is then pure arithmetic, keeping matching latency within its budget. |
| Fail-stop on exhaustion | When a block is spent, no further ids appear until the owning service refills — the platform guarantees no fabrication beyond the leased window. |
| Identity scoping | Each block carries its environment, service and sequence type, so id spaces are separated per environment and per purpose (order ids vs trade ids). |

### Transport framing and delivery hand-off

| Capability | Business rule behind it |
|---|---|
| Standard internal framing | One framing decision for the internal trading tier; services do not each invent their own — integration defects become impossible rather than merely rare. |
| Corruption containment | A broken frame degrades to a typed error; the exchange never silently processes half a message. |
| Bounded queues | Queue depth is an explicit capacity decision per hand-off, making load behavior observable and tunable rather than accidental. |
| Backpressure, not loss | A full queue hands the message back to its sender; the business rule is "pressure is an event, never a silent drop". |
| Hot-path non-blocking sends | Matching-tier traffic never waits on a queue — order latency is protected at the library level. |

## 4. Business Events (Outbound)

**Not applicable — a library emits no events and consumes none.** It holds no
database tables, no outbox and no bus connection; nothing in
`exchange8::wire` is observable on Sentin8:Bus.

The observable *effects* the library enables in consuming services (their
events, not ours):

- id-leased records appear downstream with unique, increasing business ids
  (orders in gateway/oms, trades in me, rfq trades in rfq);
- framed internal streams carry the trading-tier message flow whose lifecycle
  events (`exchange8:me:order:*`, `exchange8:me:trade:executed`, …) are emitted
  by the owning services;
- a service's failure events may reference lease exhaustion or frame decode
  errors when operators investigate — the causes, never the events, originate
  here.

## 5. Business Rules & Invariants

1. A business id issued from a lease block is unique — it can never be issued
   again by the same block, and blocks never overlap between lessees.
2. Ids hand out in strictly increasing order within a block and never reset
   within an environment across restarts or rollovers.
3. Gaps between issued ids are permitted; duplicates are structurally
   impossible.
4. When a lease block is exhausted, the library stops issuing ids — it never
   fabricates ids beyond the leased window, and the owning service decides
   between refill and fail-stop.
5. Every leased block is tied to exactly one environment, one service and one
   sequence type; id spaces never blend.
6. Corrupt, truncated or oversize traffic is always reported as a typed
   error — the system never processes a silently truncated frame.
7. A full or closing queue always returns the message to its sender; no
   internal hand-off ever loses a message silently.
8. The hot path never blocks on a queue or waits on the platform: blockage is
   surfaced as an error value for the owning service to treat.
9. Every frame obeys the 4 MiB payload bound; oversize traffic cannot force
   unbounded memory allocation.
10. The library performs no network, database, authorization or audit
    actions of its own — it operates strictly inside an already-authorized
    service process.

## 6. Target-User Roles & Use Cases

| Persona | What they do with `exchange8::wire` |
|---|---|
| Exchange service developers (Sentin8 platform engineering) | Build the shell wiring of the 13 exchange8 services: framed dispatch channels between components, envelope encoding for the ME direct streams, in-process lease blocks for business ids. |
| Matching-engine maintainers | Rely on monotonic trade-id hand-out plus a contained decode path for deterministic replay of the input journal. |
| Gateway / OMS / SOR developers | Assign order and child-lease ids from in-process blocks and exchange framed order flow with the matching engine. |
| Platform reviewers / auditors | Read a small, contract-like library whose guarantees (uniqueness, containment, backpressure-value semantics) are stated invariants rather than scattered implementation details. |
| Onboarding engineers | Learn the vertical's internal wire contract from one place instead of reverse-engineering per-service framing code. |

Representative use cases:

- *Order submission*: gateway assigns `gw_order_id` from a leased block, encodes
  the order envelope, and sends it over a bounded channel into the direct
  stream to the matching engine.
- *Trade sequencing*: the matching engine hands out trade ids from an in-process
  block; on exhaustion it fail-stops new fills until a fresh block is leased —
  no id is ever reused.
- *Component decoupling*: an OMS supervisor hands quotes to slice logic through
  a bounded queue; under a burst the excess is explicitly returned rather than
  accumulating unbounded.

## 7. KPIs / Success Criteria

| KPI | Measure | Basis |
|---|---|---|
| Id correctness | Zero duplicate or reset ids within any environment, ever | Structural (lease windows + platform-disjoint blocks), covered by in-crate unit tests. |
| Id issuance latency | In-memory (branch + increment); hot path adds no I/O | Library design goal; a lease amortizes one platform call per 10 000 ids. |
| Contained failure rate | 100% of corrupt/truncated/oversize frames surface as typed errors, with zero panics and zero silent truncation | In-crate unit tests (`frame_too_short` etc.). |
| Zero dropped work | Any rejected send returns the unclonable value to its sender | `SendError` value-returning contract, tested (`bounded_channel_backpressure`). |
| Framing consistency | One framing definition for the internal direct-stream tier across all consumers | Single codec, single consumer-wrapping client. |
| Developer integration cost | New internal service integration touches one small, documented library surface | Namespace discipline (one facade level) + documented contracts. |

## 8. Dependencies & Reliance

| Relies on | For | Consequence if unavailable |
|---|---|---|
| Sentin8 platform sequence-lease facility | The disjoint, contiguous id blocks that in-process lease windows are filled from | New business ids cannot be minted (fail-stop per owning service); in-flight trading on already-leased windows continues. |
| `serde` / `serde_json` | Payload serialization inside frames | Build-time dependency only; no runtime external service. |
| `tokio` (bounded MPSC) | The bounded queue mechanics | Build-time dependency; no runtime external service. |
| Consuming service shells | Lease refill cadence, stream wiring, authorization context | The library is inert without a host; it requires nothing beyond the process it runs in. |

Upstream in the dependency graph the crate sits beside the vertical's codec
crates and under the typed ME client; downward it depends on the platform lease
facility only through the *shells that refill blocks* — never directly from the
library itself.

## 9. Acceptance Criteria

1. A frame produced by `encode` is always decodable by `decode` to the identical
   value — proven by the in-crate round-trip test.
2. A truncated or corrupt input is always reported as a typed error
   (`TooShort` / `TooLarge` / `Json`) with no panic and no partial value.
3. An input whose declared payload exceeds 4 MiB is rejected before any
   payload copy.
4. A bounded queue under load returns undeliverable messages as values to their
   senders; FIFO order on the consumer side is preserved.
5. A lease block issues strictly increasing, unique ids; exhaustion is
   observable (`None`, zero remaining) and never yields an id beyond the block.
6. Two independently created blocks for the same identity triple can never
   overlap, because both originate from the platform facility's disjoint blocks.
7. The library remains free of network, database, token, scope or audit logic —
   reviewers can verify this in a single source file.
8. All public items are documented with usage-gated doc comments; the crate
   builds warning-free within the vertical's workspace build.

## 10. Open Business Questions

- **Lease-counter consolidation.** Three service-owned counters (trade-id,
  order-id, child-id) follow the same pattern outside the library. Is one
  shared counter type worth the cross-service refactor, or is the current
  per-service ownership acceptable as the steady state?
- **Refill observability.** Lease exhaustion and refill are currently implicit
  service-side decisions. Should block exhaustion be an auditable business
  event at the platform tier (id-allocation traceability), and which service
  owns that record?
- **Internal payload format headroom.** JSON payloads keep the internal tier
  inspectable; if internal direct-stream volume ever challenges it, when does
  the business prefer a denser binary payload over debuggability?
- **Gap reporting.** Id gaps across blocks are accepted business-wise; should
  any downstream reconciliation surface report them explicitly (e.g. for
  regulator-facing audit narratives), and where does the responsibility sit?

---

**Design attribution:** exchange8:wire and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
