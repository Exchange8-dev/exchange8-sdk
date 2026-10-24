# exchange8::sim-harness - Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> the exchange8::sim-harness library **does for the business**. For architecture
> read `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

- Let the exchange business **describe market scenarios in writing** - one
  line per action - so trading behavior (order entry, matching, cancels,
  quoting) can be specified, reviewed and replayed deterministically before
  real money touches the platform.
- Give engineering the **one command language** for simulations, conformance
  checks and performance benchmarks, keeping test vocabulary identical
  across the vertical.
- Protect the exchange's core business guarantees - fair matching, correct
  priority handling and recoverable books - by making deterministic
  scenario replay the standard verification method.

## 2. Business Scope (What the Library Provides)

| Capability | What it means for the business |
|---|---|
| Scenario script language (DSL) | Market situations - deposits, aggressive orders, cancels, replaces, sweeps, quoting, tape prints - are written as reviewable text scripts. |
| Complete trading vocabulary | Ten verbs cover the full order-lifecycle surface: new/resting orders, crossing entries, cancellations (single + all), replacements, manual prints, book sweeps, two-sided quoting, quote sets, book flush. |
| Deterministic replay | The same script always builds the same market state - reproducible matching outcomes, reliable regression and dispute analysis. |
| Cross-service scenario driving | A single script exercises the real order path (gateway acceptance → matching → clearing projection) exactly as a market day would, minus infrastructure. |
| Readable failures | A script mistake names the line and the problem - scenario maintenance costs stay low. |

### What the service does NOT provide (business boundary)

| Concern | Owning component |
|---|---|
| Actual order matching and prices | Exchange8:ME matching engine service |
| Order acceptance policy and validation | Exchange8:Gateway submission policy |
| Business record of orders/trades | Exchange8:Clearing (event-sourced ledger) |
| Market data distribution to customers | Exchange8:MD |
| Pre-trade risk checks | Exchange8:Risk |
| FIX/external venue connectivity | Exchange8:Gateway + connectivity codecs (fix/itch/mold/soup) |
| Live market operation, entitlements, audit | Exchange8 services + Sentin8 platform |

## 3. Business Capabilities

### Scenario authoring
| Capability | Business rule |
|---|---|
| Deposit / Enter (`D` / `F`) | Resting liquidity and crossing orders are expressible distinctly - a maker/taker behavior must be readable in the script itself. |
| Cancellation (`X`) | Cancelling a single order or a trader's entire book presence (optionally per symbol) is one line. |
| Replacement (`R`) | A resting order can be repriced/resized without rewriting the scenario. |
| Sweeps (`S`) | Liquidity removal up to a target price is expressible, enabling stress-style book-emptying scenarios. |
| Quoting (`2` / `Q`) | Two-sided market-maker quoting and manual consolidated quotes are first-class scenario steps. |
| Prints (`E` / `T`) | Manual trade prints and tape events are expressible for reporting-path scenarios. |
| Book flush (`Z`) | A scenario can reset the book cleanly between phases. |

### Deterministic verification
| Capability | Business rule |
|---|---|
| Script-order guarantee | Actions execute in written order - scenario outcomes are reproducible line-for-line. |
| All-or-nothing parsing | A script with a typo never partially executes - no half-built markets. |
| Human-readable failures | Every script error identifies the line, the verb or the field. |

### Cross-service assurance
| Capability | Business rule |
|---|---|
| End-to-end order path | One scenario can prove the accept → match → settle-record chain across the gateway, matching and clearing components. |
| Reject-path coverage | Invalid submissions (missing identity, zero quantity, limit without price) and halted-instrument crossings produce the standard rejection records. |
| Trade completeness invariant | Every match produces the complete per-side trade records - no half-recorded trades. |

## 4. Business Events (Outbound)

The library emits **no business events** - it is developer tooling. The
scenarios it feeds drive the real services' event streams (order lifecycle,
trade records, rejects), which those services emit and persist in the
normal way. From the business perspective the harness is a *reader and
driver* of behavior, never a source of record.

## 5. Business Rules & Invariants

1. **A scenario is exactly reproducible.** Identical script text plus
   identical engine versions always reproduce identical market outcomes.
2. **Nothing happens without a script line.** The language cannot inject
   random or time-based behavior - every market move in a simulation is
   traceable to a written action.
3. **Unknown commands are errors.** A misspelled scenario step can never be
   silently ignored; the author learns immediately.
4. **Future-proof scripts.** Adding extra parameters to existing lines is
   tolerated so scenarios remain valid as the language grows.
5. **No infrastructure dependence.** Scenarios run without databases,
   networks or credentials - verification cannot be blocked by operations.
6. **One language, many consumers.** The same script syntax serves unit
   tests, cross-service flow tests, conformance checks and future
   benchmark suites.

## 6. Target-User Roles & Use Cases

| Persona | Use of the library | Value |
|---|---|---|
| Exchange QA engineers | Write scenario scripts ordering through gateway to matching to records | Verifiable market behavior before each release. |
| Trading-operations staff (indirect) | Benefit from pre-release verification of halts, sweeps, repricing | Fewer market-day surprises. |
| Compliance & market surveillance (indirect) | Deterministic replay of disputed sequences | Evidence-grade reproduction of what happened. |
| Platform/quant engineers | Bench and replay recorded sessions against performance envelopes | Confidence in latency and throughput claims. |
| Product managers | Review scenario scripts as documentation of intended market behavior | A shared, readable language of "how our market works". |

## 7. KPIs / Success Criteria

- **Fidelity of vocabulary:** all ten documented order-handling behaviors are
  expressible in the DSL - no capability gap between spec and scenario
  language.
- **Determinism:** replaying recorded scenarios reproduces identical books,
  matches and records - measured by the flow tests staying green.
- **Failure clarity:** 100% of script errors carry line context; no silent
  scenario degradation.
- **Coverage of the order path:** a single scripted scenario proves the
  accept → match → record chain including reject paths.
- **Performance validation readiness:** harness scripts can drive the
  matching engine inside the platform's performance envelopes
  (< 100 µs P50 matching) for benchmark suites.

## 8. Dependencies & Reliance on Other Services

| Reliance on | For |
|---|---|
| `exchange8-contracts` / `exchange8-fixed` (libraries) | Order-type vocabulary and exact price arithmetic. |
| Exchange8:Gateway / Exchange8:ME / Exchange8:Clearing service cores (in tests) | The real behavior being verified - the harness replaces nothing. |
| Per-service test suites (`sql_matrix` etc.) | Tenancy/persistence guarantees the DB-free harness deliberately avoids. |
| Sentin8 platform (indirect) | Production authorization and event persistence, upstream of everything the harness verifies. |

## 9. Acceptance Criteria

1. Any exchange8 market scenario needed for regression or conformance can be
   expressed with the ten verbs without writing Rust.
2. A scenario run reproduces its outcomes exactly on every execution.
3. Script errors are reported with line numbers and never leave a partially
   applied scenario.
4. The canonical cross-service scenario demonstrates order acceptance,
   matching with complete per-side trade records, and the standard
   rejection records for invalid or halted flows.
5. Price input follows one rule everywhere in the language (scaled ticks or
   scale-4 decimals, interchangeably).
6. The scenario language contains no hidden behavior: every market effect in
   a run is attributable to a script line.

## 10. Open Business Questions

- **Session-state scenarios.** Whether market state changes (open, halt,
  reopen) should become scenario verbs so operations-visible behaviors get
  the same script-based verification as trading behaviors.
- **Instrument tables in scripts.** Whether symbol definitions belong in the
  scenario file (making scripts fully self-contained) or stay as code-side
  fixtures.
- **Benchmark scenario corpus.** Which recorded scenarios become the
  performance-validation set for the platform's latency claims, and how
  results are reported to the business.
- **Tape/print observability.** Whether manual prints should feed a readable
  "tape" output of a scenario run that ops-facing tooling can display.

---

**Architecture & Design:** exchange8:sim-harness and its underlying concepts were designed by [Hamid Jahanzad](https://www.linkedin.com/in/hamidjahanzad/).
