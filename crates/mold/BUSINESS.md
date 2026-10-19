# exchange8::mold - Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::mold library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

- Let the exchange ship its order-by-order market depth to every
  subscriber on one exact, widely-implemented multicast packet format
  (MoldUDP64) with a single tested definition.
- Keep the downstream market honest: every packet states its session and
  sequence so receivers can detect gaps, and keepalive packets advance
  the sequence so silence is distinguishable from loss.
- Make recovery cheap and standardized: receivers ask for missing ranges
  with a fixed, tiny retransmission request rather than reconnecting.
- Protect the feed from malformed or oversized packets - framing errors
  are reported, never laundered into wrong market data.

## 2. Business Scope (What the Library Provides)

| Capability | What it delivers (business terms) |
|---|---|
| Outbound packet building | Packs a session's messages (depth events) into ≤1400-byte multicast packets with correct sequence headers |
| Heartbeat packets | Count-0 packets carrying the next-expected sequence so receivers can tell "idle" from "lost" |
| Inbound packet parsing | Reads a packet into session, first sequence, count, and the byte ranges of every message block |
| Retransmission requests | Emits the 20-byte request (session, start sequence, count) used to refill missed ranges |
| Capacity management | Reports remaining packet capacity and refuses blocks that would not fit |
| Error contract | Typed failures for truncated headers and overrunning block lengths - the whole failure surface in three variants |
| Conformance tests | In-crate byte-level tests pinning header layout, sequencing, heartbeats, round-trips, and truncation behavior |

### What the service does NOT own (business boundary)

| Concern | Owned by |
|---|---|
| Message content inside blocks | `exchange8::itch` (VER2 ITCH codecs) + Exchange8:MD event mapping |
| Multicast sockets, A/B legs, heartbeat intervals | Exchange8:MD publisher / Exchange8:VFeeds session tasks |
| Retransmission storage and serving (ring buffer) | Exchange8:MD publisher service design |
| Sequencing policy (increments, snapshots from a seq) | Exchange8:MD published-session model |
| Persistence, identity, audit, event delivery | Sentin8:DB, Sentin8:Auth, Sentin8:Sys, Sentin8:Bus platform planes |

## 3. Business Capabilities

| Area | Capability | Business rule |
|---|---|---|
| Packaging | Bounded packets | A packet never exceeds 1400 bytes; a message that would not fit belongs in the next packet |
| Packaging | Session echo | Every packet names its session (10-byte padded field); receivers route by it |
| Sequencing | First-sequence stamp | A packet states the sequence of its first message; blocks are consecutive from there |
| Sequencing | Heartbeats | While idle the publisher emits count-0 packets carrying the next-expected sequence, so receivers advance their expectation |
| Recovery | Gap refill | A receiver needing missed messages requests (session, start, count); the retransmit service re-serves that exact range |
| Integrity | Reject-on-malformation | A truncated header or a block claiming more bytes than the packet contains is a typed error, never a partial decode |
| Integrity | No empty blocks | Zero-length messages are not representable and are refused at build time |

## 4. Business Events (Outbound)

- None. The library emits no events of its own. **Observable effects it
  enables:** the downstream receivers' view of
  depth (`L2` session packets), liveness (heartbeats), and repair
  (retransmission) - those arrive as events/actions owned by Exchange8:MD and
  Exchange8:VFeeds on top of this framing.

### Inbound (consumed)

- None.

## 5. Business Rules & Invariants

1. Packets are at most 1400 bytes; building beyond the cap is refused
   before encoding, never after.
2. The header is always exactly 20 bytes: 10-byte space-padded session,
   8-byte big-endian sequence, 2-byte big-endian count.
3. Each message is a 2-byte big-endian length followed by that many
   payload bytes; the declared length always fits the packet.
4. A data packet's sequence is the first message's sequence; a
   heartbeat's sequence is the next expected sequence and its count is 0.
5. An empty message payload is not encoded - the protocol's null message
   is consumed, not shipped.
6. Parsing either yields a complete, self-consistent packet view or a
   typed error; no partial repair, no default-filled fields.

## 6. Target-User Roles & Use Cases

| Persona | What they do with it |
|---|---|
| Market-data publisher engineer | Frames the depth stream into multicast packets with correct sequencing in Exchange8:MD |
| Connectivity engineer | Parses received packets and issues retransmission requests in Exchange8:VFeeds receive paths |
| QA / replay engineer | Confirms recorded packets round-trip byte-identically and truncated captures fail with documented errors |
| Platform integrator | Validates session/sequence framing before enabling a downstream consumer |

## 7. KPIs / Success Criteria

| Measure | Target |
|---|---|
| Wire fidelity | Byte-identical round-trips on the conformance suite; header layout pinned field-by-field |
| Robustness | Zero panics on network input; truncated/overrunning packets map to typed errors |
| Capacity discipline | No packet exceeds the 1400-byte bound; fit check is structural |
| Decode cost | Single-pass linear block walk per packet; no backtracking |
| Dependency footprint | No runtime dependency beyond the error-definition crate |

## 8. Dependencies & Reliance on Other Services

- **`exchange8::itch`** - produces/consumes the bytes `mold` packets
  carry; the crates are payload-agnostically decoupled (blocks are
  opaque).
- **Consuming services (Exchange8:MD publisher, Exchange8:VFeeds)** - own sockets,
  session sequencing, heartbeat timing, A/B leg strategy, and the
  retransmission server; the library cannot send or receive anything.
- **`thiserror`** - the typed failure surface.
- No reliance on Sentin8 platform planes - no identity, authorization,
  persistence, or delivery work is performed here.

## 9. Acceptance Criteria

1. A publisher-built packet parses back to the identical session,
   sequence, count, and payloads.
2. A heartbeat packet is exactly 20 bytes: session, next-expected
   sequence, count 0.
3. A retransmission request carries session, start sequence, and count
   in the fixed 20-byte layout receivers expect.
4. A packet shorter than the header, or containing a block whose
   declared length overruns it, fails with the documented distinct
   error.
5. Builder capacity accounting refuses a block that would exceed
   1400 bytes and reports remaining capacity correctly.
6. Multi-message packets record the first message's sequence once, and
   subsequent blocks are represented in order.

## 10. Open Business Questions

- **Retransmission server surface** - the request side is codec-fixed;
  the server's ring-buffer sizing and reply pacing are publisher-service
  business decisions pending that service's rollout.
- **Parse-side size policy** - whether inbound parse should itself
  enforce the 1400-byte cap or leave the policy to the receiving
  service.
- **Zero-copy receive ergonomics** - when the VFeeds receive path lands,
  a borrowed block-view API may replace offset tuples for consumer
  convenience.

---

**Design attribution:** exchange8:mold and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
