# exchange8::stamp - Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::stamp library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

- Provide the one exact, tested definition of the TSX STAMP 4.0 wire
  protocol so every exchange component - feed adapters, routers, replay
  tooling, and tests - reads and writes the same bytes.
- Protect downstream market state: the venue feed is an untrusted edge, so
  every frame must either decode faithfully or fail loudly with a typed
  error.
- Enable the market-data products built on venue quotes: consolidated
  best bid/offer (CBBO), smart order routing quotes, and routed-trade
  mirrors all rest on faithful decoding of order, trade, and session
  control messages.
- Keep exchange integration costs flat as the venue vocabulary grows: new
  tags and business classes are table entries, not format changes.

## 2. Business Scope (What the Library Provides)

| Capability | What it delivers (business terms) |
|---|---|
| STAMP frame decoding | Turns raw venue bytes into checked, ordered sections: control header, business payload, optional control trailer |
| STAMP frame encoding | Writes a decoded message back to byte-identical wire output |
| Message authoring | The `build` constructor produces order-class messages (business class + field list) for outbound flows and tests |
| Field lookup | Find the first value, an indexed (`tag.index`) value, or every value of a tag, preserving wire order |
| Wire vocabulary | The 61 numbered tags the exchange consumes and the nine order/trade business classes as named constants |
| Error contract | Three typed, display-formatted failures (missing control header, missing `=` separator, non-UTF-8 bytes) |
| Conformance tests | Twelve in-crate byte-level tests pinning framing, sections, indexed fields, business classes, and round-trip identity |

### What the service does NOT own (business boundary)

| Concern | Owned by |
|---|---|
| Transport/session (SoupBinTCP logins, heartbeats, sequenced data) | `exchange8::soup`, Exchange8:VFeeds session tasks |
| Multicast packaging and gap recovery | `exchange8::mold` + Exchange8:VFeeds A/B recovery |
| Quote normalization (bytes → internal quote events, price scaling) | Exchange8:VFeeds / Exchange8:SOR adapter layers |
| Order routing decisions | Exchange8:SOR |
| FIX 4.2 order-entry encoding | `exchange8::fix`, Exchange8:Gateway |
| Persistence, identity, audit, event delivery | Sentin8:DB, Sentin8:Auth, Sentin8:Sys, Sentin8:Bus platform planes |

## 3. Business Capabilities

| Area | Capability | Business rule |
|---|---|---|
| Decoding | Session messages (header-only frames) | Heartbeat and session-control frames decode from the control header alone; empty business content is expected, not an error |
| Decoding | Business messages | The business section carries the class discriminator (order request, ack, cancel, trade confirm…) plus the domain fields, in wire order |
| Decoding | Multi-value fields | A field can repeat (`50.1`, `50.2`, …); every occurrence is kept, never merged |
| Encoding | Round-trip fidelity | What is decoded can be re-emitted byte-for-byte - no reordering, no rewriting |
| Authoring | Outbound construction | A message is built from a business class plus name/field pairs; the class is always the first business field |
| Vocabulary | Tag constants | Tag numbers are the venue's numbering; the crate names the exact subset the exchange consumes |
| Error contract | Rejections | Malformed input produces one of three typed errors; the codec never repairs or defaults silently |

## 4. Business Events (Outbound)

- None. The library emits no events, holds no state, and participates in
  delivery only through its callers: quote events (`bbo`, venue quotes),
  trade mirrors, and feed-status signals belong to the services whose
  adapters call the codec.

### Inbound (consumed)

- None. The codec is a pure function of its inputs; it subscribes to
  nothing and schedules nothing.

## 5. Business Rules & Invariants

1. A STAMP frame always begins with the control-header byte; frames that
   do not are rejected as malformed, never guessed around.
2. The business section and control trailer are optional: session-only
   frames (header only) and truncated captures decode without failure.
3. Field order is preserved end-to-end: decode never re-sorts, encode
   never reorders.
4. Multi-value fields keep their index suffix; an unindexed field and a
   `tag.1` field are distinct identities.
5. Values travel verbatim: no trimming, no case folding, no numeric
   reinterpretation inside the codec; consumers interpret values.
6. Round-trip fidelity: for grammar-conformant input, decode→encode
   reproduces the original bytes exactly.
7. Malformed input surfaces as a typed error (missing header, missing `=`,
   non-UTF-8) rather than a panic or a silently repaired message.

## 6. Target-User Roles & Use Cases

| Persona | What they do with it |
|---|---|
| Market-data adapter engineer | Decodes STAMP CBBO/quote frames inside Exchange8:VFeeds adapters; encodes test fixtures |
| Trading-connectivity engineer | Builds router-side consumers (Exchange8:SOR) that read consolidated quotes and author order-class messages in tests |
| QA / replay engineer | Verifies recorded venue captures decode to identical bytes and that malformed frames produce the documented error |
| Platform integrator | Confirms venue compatibility against the pinned tag and business-class tables before enabling a feed |

## 7. KPIs / Success Criteria

| Measure | Target |
|---|---|
| Round-trip fidelity | 100% byte-identical on the conformance suite's corpus |
| Conformance coverage | Framing constants, section splitting, indexed fields, business classes, and error taxonomy all pinned by tests (12 in-crate tests) |
| Robustness | Zero panics on grammar-conformant input; every detectable malformation mapped to a typed error |
| Decode cost | Single-pass linear scan per frame; no backtracking, no re-sorting |
| Dependency footprint | No runtime dependencies beyond the error-definition crate; no network or database surface |

## 8. Dependencies & Reliance on Other Services

- **The venue (TSX/TSXV)** - the protocol authority: tag numbering, class
  codes, and framing grammar are the venue's wire contract.
- **Consuming services (Exchange8:VFeeds, Exchange8:SOR)** - own transport, session
  supervision, and the translation of decoded fields into business events;
  the library cannot emit anything business-shaped by itself.
- **Adjacent codec crates** - `exchange8::soup` and `exchange8::mold` frame
  the session and packet layers STAMP bytes ride in; `exchange8::fixed`
  owns the price/quantity arithmetic that decoded text values feed.
- **Error-definition crate (`thiserror`)** - the typed failure surface.
- No reliance on Sentin8 platform planes - the library performs no
  identity, authorization, persistence, or delivery work.

## 9. Acceptance Criteria

1. Every recorded venue frame in the conformance corpus decodes without
   error and re-encodes byte-identically.
2. A header-only heartbeat frame, a header-plus-trailer frame, and an
   empty-section frame all decode; none is treated as a protocol violation.
3. Repeated (`tag.1`, `tag.2`, …) fields arrive with every value and their
   original order intact.
4. A frame missing its control-header byte, a field missing its `=`
   separator, and a field carrying non-UTF-8 bytes each fail with the
   documented, distinct error.
5. A constructed order-class message serializes with the class
   discriminator as the first business field.
6. No decoding path panics, defaults, or reorders fields - including for
   hostile input at the section/field grammar level.

## 10. Open Business Questions

- **Typed message products.** Do consumers want per-class typed messages
  (order request, trade confirm) with schema-enforced field lists, or do
  adapter layers continue to own interpretation?
- **Trailer-level controls.** Should the exchange accept frames whose
  control trailer fails venue-level integrity/authenticity checks once
  trailer semantics are pinned, or reject at decode?
- **First direct consumer.** Which service ships the first production
  adapter bound to this codec: the feeds adapter (`stamp_cbbo` payload
  kind) declared in the venue-feed design, or the router's feed path?
- **Order-entry direction.** The codec can author order-class messages -
  is inbound STAMP order entry in scope for the exchange, or is STAMP
  decode-only (quote/status) for the foreseeable product?

---

**Architecture & Design:** exchange8:stamp and its underlying concepts were designed by [Hamid Jahanzad](https://www.linkedin.com/in/hamidjahanzad/).
