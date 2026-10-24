# exchange8::me::client - Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::me::client library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

- Give every exchange8 order-flow participant (Gateway, OMS, ME replicas) a
  **single, identical way to speak to the matching engine** over direct
  streams, so order-entry traffic is consistent, auditable and fast.
- Protect the trading hot path's business guarantees - **latency,
  deterministic ordering and exactly-once processing** - by making the typed
  message envelope (tenant, sequence, idempotency key) inseparable from every
  message sent to or received from the ME.
- Prevent divergence bugs with direct commercial impact (mis-routed fills,
  duplicated trades, lost tenant attribution) by taking frame-encoding decisions
  away from individual services.

## 2. Business Scope (What the Library Provides)

| Capability | What it means for the business |
|---|---|
| Typed ME-stream encoding | Every order-entry, lifecycle and recovery message reaching the matching engine is serialized the same way, by every participant, every time. |
| Typed ME-stream decoding | Services reading ME responses (execution confirmations, rejects, state recovery) get fully typed, tenant-stamped messages - never raw JSON. |
| Complete envelope fidelity | Tenant environment (`env_id`), per-emitter sequence (`seq`) and idempotency key travel with every message, enabling auditable sequencing and duplicate-proof processing. |
| Contained error behavior | Corrupt or oversize stream input surfaces as a clean typed error, so a bad frame never silently drops money-affecting information. |
| Determinism of the stream format | Recorded sessions replay byte-identically - the basis for crash recovery and dispute reconstruction. |

### What the library does NOT provide (business boundary)

| Concern | Owning component |
|---|---|
| Establishing streams, TLS certificates, reconnects | Sentin8:TLS transport + consumer service shells |
| Granting access to trade (entitlement, limits) | Exchange8:Risk + Sentin8:Auth authorization pipeline |
| Deciding trade validity, matching, prices | Exchange8:ME (matching engine service) |
| Persisting orders/trades for the business record | Exchange8:Clearing via Sentin8:Bus (inbox-first mirror) |
| Defining the meaning of order/trade messages | `exchange8::contracts` (typed event catalog) |
| Queue backpressure and id-lease blocks | `exchange8::wire` transport primitives |

## 3. Business Capabilities

### Order-entry stream encoding
| Capability | Business rule |
|---|---|
| Envelope-first encoding | A message is never sent to the ME without its tenant, sequence and idempotency identity - no anonymous or untraceable trade traffic. |
| One frame format | All participants (GW, OMS, ME, replicas) produce compatible frames; an operationally added service speaks the same stream on day one. |

### Downstream stream consumption
| Capability | Business rule |
|---|---|
| Typed reads | Consumers of ME executions/rejects work with fully typed business objects, reducing integration defects in the order lifecycle. |
| Error containment | A malformed or oversized frame never panics or partially applies; the failure is detectable and recoverable at the consumer. |

### Stream-format assurance
| Capability | Business rule |
|---|---|
| Format determinism | Identical business input always yields identical on-the-wire bytes, keeping crash recovery, replay and audit reconstruction trustworthy. |
| Transport portability | The same message body serves the direct trading stream and the durable bus mirror - no business meaning is lost between tiers. |

## 4. Business Events (Outbound)

The library emits **no business events** - it is in-process plumbing. The
envelopes it encodes/decodes are the messages of the trading hot path:
order entry and lifecycle (`exchange8:me:order:*`, `exchange8:me:trade:*`
families) exchanged point-to-point between Gateway/OMS and ME. A mirrored
copy of the same message bodies reaches durable persistence/audit consumers
(Clearing, MD, Margin, Settlement, Regulatory) through Sentin8:Bus, where
event-driven bookkeeping happens.

## 5. Business Rules & Invariants

1. **Identity travels with the trade.** No message reaches the ME without
   its environment, sequence number and idempotency key.
2. **One format, no exceptions.** Every ME-stream frame comes from one
   encode path; hand-rolled framing is impossible to reach from a compliant
   consumer.
3. **Failure is loud, never silent.** Corrupt input returns an explicit
   error; no value is truncated, defaulted or dropped quietly.
4. **No credentials in the library.** The library never holds tokens,
   certificates or secrets - access control stays with the platform shell.
5. **Round-trip fidelity.** What a service sends is exactly what the peer
   decodes - byte-for-byte identical semantics on both sides.
6. **Deterministic replay.** The stream format supports byte-identical
   replay for recovery and dispute reconstruction.

## 6. Target-User Roles & Use Cases

| Persona | Use of the library | Value |
|---|---|---|
| Exchange platform engineer (Gateway/OMS/ME teams) | Encode/decode typed envelopes on direct streams | One shared client surface across services; fewer integration incidents. |
| Trading-operations staff (indirect) | Benefit from consistent executions, rejects and recovery | Fill/reject notices arrive reliably and traceably. |
| Compliance & audit (indirect) | Rely on tenant/sequence/idempotency discipline of every stream message | Order flow is fully attributable and replayable. |
| Quant/QA engineers | Replay recorded sessions; run deterministic flow tests (with the sim harness) | Reproducible verification of trading behavior before market open. |

## 7. KPIs / Success Criteria

- **Reliability:** zero silent-corruption events on ME streams; all framing
  failures produce explicit, actionable errors.
- **Latency:** message codec cost stays negligible against the matching
  latency envelope (sub-microsecond encode/decode at hot-path message sizes).
- **Interoperability:** any new order-flow service integrates the ME stream
  without format negotiation or bespoke codecs.
- **Recoverability:** crash recovery and replay reconstruct byte-identical
  streams from the recorded history.
- **Security posture:** no credential or secret handling ever appears in the
  library (verified by the absence of such surfaces in its API).

## 8. Dependencies & Reliance on Other Services

| Reliance on | For |
|---|---|
| `exchange8::wire` (library) | The frame byte layout, size bounds and error taxonomy. |
| `exchange8::contracts` (library) | The typed envelope and all order/trade message definitions. |
| Sentin8:TLS + Sentin8:Auth (platform) | Authenticated, certificate-bound stream transport. |
| Consumer service shells (Gateway, OMS, ME) | Stream lifecycle: connect, reconnect, backpressure, recovery policy. |
| Exchange8:Clearing via Sentin8:Bus | Durable business record of everything that flowed on a stream. |

## 9. Acceptance Criteria

1. A Gateway or OMS engineer can put a fully typed envelope onto the ME
   stream and read a typed response back with exactly two function calls.
2. Every stream message carries environment, sequence and idempotency
   identity without any caller-side assembly of those fields.
3. Corrupt, truncated or oversized input produces a typed error - never a
   panic, truncation or undefined behavior.
4. Round-trip (`encode` → `decode`) is lossless for all trading payloads.
5. The library contains no network, database, token or secret surface.
6. Recorded ME sessions replay byte-identically through this codec.

## 10. Open Business Questions

- **Per-family typed stream products.** Business-facing message sets per
  stream family (order entry vs. lifecycle vs. recovery) could be formalized
  as named types, tightening contracts between trading participants; whether
  the added ceremony helps non-engineering stakeholders is undecided.
- **Reader-side sequence guarantees.** Whether "next expected sequence"
  bookkeeping for stream readers should become a shared business guarantee
  (uniform gap detection and reporting across consumers) or remain a
  per-service policy.
- **Replay tooling as an operational product.** Whether recorded-session
  replay should expose a supported operational interface (e.g. for dispute
  reconstruction requests) rather than remain an engineering test facility.

---

**Architecture & Design:** exchange8:client and its underlying concepts were designed by [Hamid Jahanzad](https://www.linkedin.com/in/hamidjahanzad/).
