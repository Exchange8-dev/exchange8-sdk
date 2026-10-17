# exchange8::soup — Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::soup library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

- Give market-data subscribers a standard session protocol
  (SoupBinTCP) over which they authenticate, request replay from a
  sequence position, and receive the live tape — with one tested byte
  definition on the platform side.
- Make the handshake an exact contract: credentials, requested session,
  and replay-start sequence travel in fixed-width fields every
  established client library already knows how to fill.
- Keep sessions observable and healthy: heartbeats in both directions
  and clean logout frames let publishers and receivers maintain idle
  connections without protocol ambiguity.
- Wrap the published stream so a session is self-consistent — the
  server's login accept names the session and the exact first sequence
  the client will receive.

## 2. Business Scope (What the Library Provides)

| Capability | What it delivers (business terms) |
|---|---|
| Login request encoding/decoding | The 49-byte client handshake: username, password, requested session, replay-from sequence — typed struct both directions |
| Login accept encoding/decoding | The 33-byte server grant: session name and the sequence the stream starts at |
| Login rejection frames | Exact short rejection frames with the two reason codes (not authorized / session unavailable) |
| Heartbeat frames | Server and client 3-byte keepalives for idle connection supervision |
| Logout frame | The client's clean-disconnect 3-byte frame |
| Sequenced data wrapping | Wraps a market-data payload (ITCH bytes) into the `S` data frame |
| Stream read support | Length-prefix lookahead (`next_frame_len`) and frame splitting (`split_frame`) for any byte-stream reader |
| Error contract | Typed failures for short frames, wrong login payloads, and unknown types |
| Conformance tests | In-crate golden tests pinning the handshake layout, control frames, wrapping, and partial-frame reads |

### What the service does NOT own (business boundary)

| Concern | Owned by |
|---|---|
| Socket/TCP connection management | Exchange8:MD / Exchange8:VFeeds session tasks |
| Heartbeat intervals and drop policy | Session tasks (platform config) |
| Credential verification and authorization policy | Sentin8:Auth / Sentin8:Proxy at the service shell |
| Payload content inside data frames | `exchange8::itch` + Exchange8:MD publisher |
| Replay persistence and sequencing | Exchange8:MD published-session model (DB-backed replay) |
| Persistence, identity, audit, event delivery | Sentin8:DB, Sentin8:Auth, Sentin8:Sys, Sentin8:Bus platform planes |

## 3. Business Capabilities

| Area | Capability | Business rule |
|---|---|---|
| Authentication surface | Fixed-width handshake | A subscriber's request carries exactly 6-byte user, 10-byte password, 10-byte session, and a 20-character right-aligned sequence number — over-length input truncates per protocol contract |
| Session establishment | Accept grant | The server answers with the session and the starting sequence; the client resumes exactly there |
| Access control signaling | Rejection | Two reasons: not authorized, or session not available — distinct, explicit, typed |
| Liveness | Heartbeats | Either side emits a 3-byte keepalive; silence beyond the interval is the session task's disconnect policy |
| Clean exit | Logout | A client requests disconnection with one unambiguous frame |
| Data delivery | Sequenced data | Market-data payloads ride inside `S` frames; the framing layer treats content as opaque |
| Robustness | Exact-length handshake | A login payload of any other length is a typed error — never truncated or guessed |
| Robustness | Stream discipline | A reader knows how many bytes make a complete frame before touching payload content |

## 4. Business Events (Outbound)

- None. The library emits no events and holds no state. **Observable
  effects it enables:** subscriber sessions (authentication, replay from
  a historical sequence, live continuation, liveness) — those are
  Exchange8:MD / Exchange8:VFeeds behaviors on top of these frames.

### Inbound (consumed)

- None.

## 5. Business Rules & Invariants

1. Every message shares one frame grammar: 2-byte big-endian length
   prefix, then 1 type byte, then payload (length includes the type
   byte, excludes the prefix).
2. The login request payload is exactly 47 bytes and the login accept
   exactly 31; other lengths are typed rejections.
3. Control frames (heartbeats, logout) are exactly 3 bytes; the login
   rejection is exactly 4.
4. Credentials and session fields are blank-padded to fixed widths
   and blanks-trimmed on parse — inputs longer than the width truncate
   by protocol contract.
5. The sequence travels as a 20-character right-aligned decimal string;
   non-numeric text decodes tolerantly to 0 rather than failing the
   session.
6. Rejection reasons are exactly the two codes `A` (not authorized) and
   `S` (no session).

## 6. Target-User Roles & Use Cases

| Persona | What they do with it |
|---|---|
| Market-data session engineer | Builds the Soup publisher side: accept, heartbeats, replay-from-login in Exchange8:MD |
| Connectivity engineer | Builds subscriber-facing receive loops that parse inbound frames into (type, payload) |
| QA / replay engineer | Verifies recorded handshake and data frames round-trip and that malformed lengths fail typed |
| Platform integrator | Confirms handshake field offsets before enabling a subscriber class |

## 7. KPIs / Success Criteria

| Measure | Target |
|---|---|
| Wire fidelity | Byte-exact handshake and control frames, pinned by golden tests |
| Robustness | No panics on network input; malformed login lengths typed-rejected |
| Round-trip | Login request/accept decode reproduces the encoded struct exactly |
| Decode cost | Constant-time length lookahead; single-pass split |
| Dependency footprint | No runtime dependency beyond the error-definition crate |

## 8. Dependencies & Reliance on Other Services

- **`exchange8::itch`** — the market-data payloads this framing wraps in
  sequenced data.
- **Consuming services (Exchange8:MD, Exchange8:VFeeds)** — own sockets, heartbeat
  schedules, reconnection, replay reads, and credential verification;
  the library cannot connect, listen, or authenticate.
- **`thiserror`** — the typed failure surface.
- No reliance on Sentin8 platform planes — no identity, authorization,
  persistence, or delivery work is performed inside the library.

## 9. Acceptance Criteria

1. An encoded login request is exactly 49 bytes with user, password,
   session, and 20-digit sequence at the documented offsets, and parses
   back to the identical struct.
2. A login accept is exactly 33 bytes and round-trips session +
   sequence.
3. Server heartbeat, client heartbeat, and logout frames are exactly
   3 bytes with the correct type letters; the rejection frame is 4.
4. A sequenced-data frame wraps any payload with length `2 + 1 + payload`
   and its prefix predicts the total frame length.
5. A login payload of any length other than 47 (or accept other than
   31) is rejected with the typed length error.
6. A buffer shorter than two length bytes reports "needs more bytes"
   rather than an error or a guess.

## 10. Open Business Questions

- **Unsequenced-data (`U`) flows** — whether any subscriber class will
  submit data upstream, which would add an encode/decode pair for that
  frame family.
- **Sequence decode strictness** — the tolerant `0` fallback on
  non-digit text is current policy; whether callers must distinguish
  "client asked for 0" from "client sent garbage" is undecided.
- **Additional classic-Soup event types** (end-of-session, debug/debug-
  plus) are represented only by knowledge of their letters; supporting
  them awaits a consumer need.
- **Compression / auth extensions** — the codec implements the classic
  fixed handshake; any platform decision to extend authentication (e.g.
  token-carrying login fields) is a session-service design, not this
  wire contract.

---

**Design attribution:** exchange8:soup and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
