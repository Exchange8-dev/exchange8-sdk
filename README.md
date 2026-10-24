# exchange8-sdk - Exchange8 Vertical SDK
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](../LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Docs](https://img.shields.io/badge/docs/read-3aa757.svg)](https://exchange8.dev/docs/services/sdk)
[![Website](https://img.shields.io/badge/website-exchange8.dev-1a73e8.svg)](https://exchange8.dev/docs/sdk)

## 1. Summary

The Exchange8 vertical's **shared plane** - the vertical's own
`sentin8-sdk` equivalent (SDD §3.4): fixed-point math, market
microstructure contracts, and exchange wire protocols that EVERY
exchange8 service (matching engine, MD, gateway, clearing, …)
links. It stays one workspace because dissolving it would recreate
cross-service repo cycles one level down.

## 2. Crates

| Crate | Purpose |
|---|---|
| `facade` (package `exchange8`) | Namespace facade - `use exchange8::fixed::…` |
| `crates/fixed` | `Price`/`Qty` fixed-point decimal math |
| `crates/contracts` | Shared event/wire types - orders, fills, enums |
| `crates/me/client` | Matching-engine client |
| `crates/book` | Order-book primitives |
| `crates/fix` `crates/itch` `crates/soup` `crates/mold` `crates/stamp` | Market wire protocols (FIX, ITCH, SOUP, MoldUDP64, STAMP) |
| `crates/wire` | Internal exchange wire framing |
| `crates/sim-harness` | Simulation harness for load/soak tests |

## 3. Consumption

```toml
exchange8 = { git = "https://github.com/Exchange8-dev/exchange8-sdk", tag = "exchange8-v0.1.0" }
```

The facade pins the platform SDK (`sentin8` - shared plane only,
SDD §2.2).

---

**Architecture & Design:** exchange8:sdk and its underlying concepts were designed by [Hamid Jahanzad](https://www.linkedin.com/in/hamidjahanzad/).
