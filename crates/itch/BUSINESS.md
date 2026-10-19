# exchange8::itch - Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::itch library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

- Publish the exchange's market state to the outside world in the venue's
  own market-data language: the VER2 ITCH binary (and text-mode) packet
  formats, so every downstream consumer - from professional terminals to
  retail gateways - reads exactly what the engine did.
- Preserve market semantics through the wire: order identity, broker
  attribution, book identifiers (lit / dark / QLX / MEMP), match numbers,
  and cross types survive to the last byte.
- Give downstream participants a complete order-event narrative - adds,
  executions, replacements, cancels, deletes, trades, crosses, busts,
  trading states, directory entries, timestamp markers - in one codec.
- Keep the published tape byte-stable: message shapes, lengths, and pads
  are pinned by the wire catalog, so platform upgrades never surprise a
  subscriber.

## 2. Business Scope (What the Library Provides)

| Capability | What it delivers (business terms) |
|---|---|
| Binary ITCH encoding | Order events, trades, crosses, busts, directory, trading states, system events, and timestamps as exact-length wire messages |
| ITCH text-mode encoding | The classic fixed-width text forms of add/execute/trade/cancel/delete - including the 10-wide "large" variants when numbers overflow narrow fields |
| Message-length contract | Field-by-field length and type tables the publisher's framing (MoldUDP64 blocks / Soup frames) relies on |
| Field decode helpers | Safe `u16`/`u32` big-endian reads and a type→length table for stream walk-throughs |
| Wire vocabulary | Message-type letters, book-id letters, padding bytes, and width-threshold constants as named identifiers |
| Error contract | Typed failures for short buffers and unknown message types |
| Conformance tests | In-crate golden byte tests anchoring every message layout to the wire catalog |

### What the service does NOT own (business boundary)

| Concern | Owned by |
|---|---|
| Event→message policy (which event becomes which message) | Exchange8:MD book/projector layer |
| Packet framing and multicast transport | `exchange8::mold`, `exchange8::soup` + session tasks |
| Price/quantity numeric semantics | `exchange8::fixed` (wire `u32` ticks convert at the boundary) |
| Broker anonymization / suppression policy | Exchange8:MD per the platform's participant-privacy rules |
| Sequencing, replay, and retransmission | Exchange8:MD publisher session model |
| Persistence, identity, audit, event delivery | Sentin8:DB, Sentin8:Auth, Sentin8:Sys, Sentin8:Bus platform planes |

## 3. Business Capabilities

| Area | Capability | Business rule |
|---|---|---|
| Order visibility | Add Order (`A`) | Adds a visible order: side (Buy/Sell), instrument, order id, size, price, broker, book - dark/hidden orders never reach the feed (policy upstream) |
| Order progress | Order Executed (`E`) | Reports a fill against an order id, with contra broker and the platform's match number |
| Order change | Order Replaced (`U`) | Carries original and new order ids with new size/price |
| Order removal | Order Cancel (`X`) / Delete (`D`) | Cancel trims a stated quantity; delete removes the remainder instantly |
| Trade reporting | Trade (`P`) / Cross Trade (`Q`) | Prints trades with both broker sides, book, bypass flag, and preserved cross types (opening/closing/intermarket/various/midpoint semantics upstream) |
| Trade integrity | Broken Trade (`B`) | Busts a previously printed trade by match number |
| Instrument state | Trading Action (`H`) / Directory (`R`, `r`) | Publishes trading states (halted/paused/quoting) and reference data at start of day |
| Time markers | Seconds (`T`) / Milliseconds (`M`) | Timestamp markers the session emits between event bursts |
| System clock | System Event (`S`) | Market-phase signals (start/startup/open/quoting/end/post) preserved by code |
| Text-mode reach | Text-mode encoders | The space-padded text forms (41/45, 29/33, 52/56, 16/20, 10 byte-lengths) that text-mode receivers parse - width-expanding automatically when sizes exceed six digits |
| Robustness | Golden conformance | Every binary and text layout is pinned byte-level against the wire catalog inside the crate |

## 4. Business Events (Outbound)

- None. The library emits no events and holds no state. **Observable
  effects it enables:** the public market tape and snapshot replay that
  downstream subscribers consume - those arrive as Exchange8:MD / Exchange8:VFeeds
  behaviors riding this codec's bytes.

### Inbound (consumed)

- None.

## 5. Business Rules & Invariants

1. Every binary message occupies its exact catalog length (from 4 bytes
   for system event to 61 for the extended directory); encoders assert
   their length in debug builds.
2. All multi-byte numbers are big-endian; all reserved padding is
   space bytes (0x20).
3. The message type letter is always byte 0 of any binary or text form.
4. Text-mode numeric fields are right-aligned, blank-padded: 9-wide
   order ids and references, 6-wide share counts - expanding to 10-wide
   when a size exceeds 999 999 (message length widens with it).
5. Text symbol fields are left-aligned and clamped to 10 characters.
6. Broker codes, book identifiers, and cross/bypass codes travel as
   single letters; the platform's anonymization runs before encoding.
7. An unknown message type has no length and is a typed error - length
   tables never guess.

## 6. Target-User Roles & Use Cases

| Persona | What they do with it |
|---|---|
| Market-data engineer | Maps book events to ITCH messages in Exchange8:MD; builds the L1/L2 published payload |
| Connectivity engineer | Uses the length table to parse a session stream or verify an external consumer's capture |
| QA / replay engineer | Verifies recorded messages byte-match the catalog and reconstruct expected lengths |
| Downstream integrator | Reads the pinned type/length vocabulary to build reliable book builders against the feed |

## 7. KPIs / Success Criteria

| Measure | Target |
|---|---|
| Wire fidelity | Encoders produce catalog-exact lengths; golden tests pin every layout |
| Robustness | Encode cannot produce an invalid length; unknown types are typed errors, never guesses |
| Coverage | All fourteen binary message types and the five text-mode families encoded; decode helpers available where consumers need them |
| Determinism | Same struct → same bytes on every host |
| Dependency footprint | No runtime dependency beyond the error-definition crate |

## 8. Dependencies & Reliance on Other Services

- **Exchange8:MD** - the only code consumer today: the book projector maps
  internal events onto this crate's structs and bytes (order adds,
  executions, cancels, deletes, replaces).
- **Framing crates** - `exchange8::mold` (multicast packets) and
  `exchange8::soup` (session frames) wrap ITCH bytes for transport.
- **Design truth anchor** - the wire-formats behavior catalog pins byte
  layouts, type letters, and text widths; the crate mirrors it under
  test.
- No reliance on Sentin8 platform planes - no identity, authorization,
  persistence, or delivery work is performed here.

## 9. Acceptance Criteria

1. Each message's encoder output matches the catalog length and
   field order exactly, byte-by-byte in golden tests.
2. Text-mode encoders emit blank-padded right-aligned numerics, 10-wide
   symbols, and the specified total lengths - narrow and wide variants.
3. The length table returns the documented size for every type letter
   and `None` for unknown ones.
4. Reserved padding bytes are spaces; the type byte is first.
5. Every struct survives a decode spot-check by its accessor helpers
   (`read_u16`/`read_u32`) in tests.

## 10. Open Business Questions

- **Full binary decode structs** - today encoders exist for the fourteen
  binary types plus safe word readers; whether per-field decode structs
  for directory/extended types should be added in-crate is pending the
  receive-path consumer's needs.
- **Text-mode receiver support matrix** - which consumers still require
  text forms (and their width quirks) is an integration census; encoders
  remain wired for them until policy retires them.
- **Timestamp cadence** - seconds/millis marker emission is a publisher
  policy choice (Exchange8:MD), not an encoding one.
- **Additional directories** - book-id/venue-specific directory
  extensions beyond the catalog may need a vocabulary pass-through
  decision.

---

**Design attribution:** exchange8:itch and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
