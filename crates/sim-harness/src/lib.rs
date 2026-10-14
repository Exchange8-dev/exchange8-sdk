// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 market simulator + scenario DSL.
//!
//! Parses the platform spec tool command language verbatim — 1:1 verb
//! parity: `D` deposit/new order, `X` cancel, `Z` flush book, `F`
//! aggressive enter, `R` replace, `E` manual exec print, `S` sweep,
//! `2` two-sided quote, `T` tape print, `Q` quote set. Line syntax:
//! `VERB key=value ...` (`#` starts a comment line).
//!
//! The crate is parse-only: it models scenarios, it never drives an
//! engine itself. Drivers (test binaries, tools) `#[path]`-include
//! service-core modules and apply the parsed actions — the canonical
//! pattern is `tests/flow_gateway_me_clearing.rs` (gateway submission →
//! ME matching → clearing projection, DB-free).
//!
//! # Quickstart
//!
//! ```
//! use exchange8_sim_harness::{parse_script, Action};
//!
//! let script = "\
//! D trader=alice sym=ACME side=B qty=100 px=10.50
//! F trader=bob sym=ACME side=S qty=100 px=10.50 tif=ioc
//! ";
//! let actions = parse_script(script).unwrap();
//! assert!(matches!(actions[0], Action::Deposit(_)));
//! assert!(matches!(actions[1], Action::Enter(_)));
//! ```
//!
//! Design: `crates/sim-harness/DESIGN.md` (detailed design);
//! `design/sentin8-exchange8-technical-design.md` §11 (testing,
//! determinism and replay).

use exchange8_contracts::{OrdType, OrderSide, Tif};
use exchange8_fixed::Price;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One parsed simulation action.
///
/// Actions are produced in script order by [`parse_script`] and applied by
/// a driver (a test binary or tool) that maps them onto real service-core
/// calls — the crate attaches no execution semantics of its own. The enum
/// is serde-stable (`Serialize`/`Deserialize`) and `PartialEq`, so
/// golden-script assertions compare whole action plans directly and
/// checked-in scenario files stay replayable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// New resting order.
    Deposit(OrderRequest),
    /// Aggressive (crossing) order.
    Enter(OrderRequest),
    /// Cancel every order of one trader, optionally filtered by symbol.
    CancelAll {
        /// Trader whose outstanding orders are flushed.
        trader: String,
        /// Restrict the cancel to this symbol; `None` = all symbols.
        symbol: Option<String>,
    },
    /// Cancel one order of one trader by engine order id.
    Cancel {
        /// Trader who owns the order.
        trader: String,
        /// Engine order id (harness `oid=` parameter).
        order_id: u64,
    },
    /// Replace one order with a newly built request.
    Replace {
        /// Trader who owns the order.
        trader: String,
        /// Engine order id being replaced (harness `oid=`).
        order_id: u64,
        /// The replacement request (always a resting day-order).
        new: Box<OrderRequest>,
    },
    /// Manual execution print (no book effect; drivers render it).
    Print {
        /// Instrument symbol the print is booked on.
        symbol: String,
        /// Scaled print price (ticks).
        price: i128,
        /// Printed quantity.
        qty: u64,
    },
    /// Sweep the book on one side up to a target price.
    Sweep {
        /// Instrument symbol whose book is swept.
        symbol: String,
        /// Book side swept (`'B'` bids, `'S'` asks).
        side: char,
        /// Scaled target price the sweep runs to.
        to_px: i128,
    },
    /// Two-sided auto-quote from one simulated participant.
    TwoSided {
        /// Quoting trader.
        trader: String,
        /// Instrument symbol quoted.
        symbol: String,
        /// Scaled bid price.
        buy_px: i128,
        /// Scaled ask price.
        sell_px: i128,
        /// Per-side quote size.
        qty: u64,
    },
    /// Set a manual consolidated quote on one symbol.
    Quote {
        /// Instrument symbol quoted.
        symbol: String,
        /// Scaled bid; `None` removes the bid side.
        bid: Option<i128>,
        /// Scaled ask; `None` removes the ask side.
        ask: Option<i128>,
    },
    /// Flush the book (the `Z` verb; the `T` tape print is the same no-op).
    Flush,
}

/// Order request built by the DSL (`D`, `F`, `R` verbs).
///
/// Plain data — no behavior — so drivers can compare parsed plans directly
/// and build engine-bound structures (`AddOrder`, `ExchangeOrder`) without
/// any harness-side interpretation of the fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderRequest {
    /// Simulated participant who owns the order.
    pub trader: String,
    /// Instrument symbol the order trades.
    pub symbol: String,
    /// `'B'` buy, `'S'` sell (spec-faithful; [`side_of`] maps to
    /// [`OrderSide`]).
    pub side: char,
    /// Ordered quantity in units.
    pub qty: u64,
    /// Scaled price (ticks); `None` = market.
    pub price: Option<i128>,
    /// Time-in-force (`ioc`/`fok` spelled out, otherwise day).
    pub tif: Tif,
}

/// Contained scenario-parse failure — always names the 1-based line.
///
/// [`parse_script`] never panics: a malformed line stops the whole script
/// (all-or-nothing, so a driver never runs a partially parsed plan) and
/// the error names the offending verb, field or value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// The line's first token is not one of the ten spec verbs.
    #[error("line {line}: unknown verb {verb:?}")]
    BadVerb {
        /// 1-based line number of the offending line.
        line: usize,
        /// The unknown verb, as written in the script.
        verb: String,
    },
    /// A verb-required parameter is absent from the line.
    #[error("line {line}: missing field {field}")]
    MissingField {
        /// 1-based line number of the offending line.
        line: usize,
        /// The missing DSL field name (e.g. `sym`, `qty`).
        field: &'static str,
    },
    /// A numeric field parses as neither scaled ticks nor a decimal price.
    #[error("line {line}: bad number {got:?}")]
    BadNumber {
        /// 1-based line number of the offending line.
        line: usize,
        /// The offending value, as written in the script.
        got: String,
    },
}

/// Get a DSL parameter value if present.
fn getp<'a>(params: &'a [(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// Integer or decimal price text → scaled ticks.
///
/// One scaling rule for the whole DSL: a bare integer is already-scaled
/// (`105000` stays `105_000`); any other text parses as a scale-4 decimal
/// (`10.50` → `105_000`) via `exchange8_fixed::Price::parse_decimal`.
fn parse_px(s: &str) -> Option<i128> {
    if let Ok(v) = s.parse::<i128>() {
        return Some(v); // bare int = already scaled
    }
    Price::parse_decimal(s, 4).map(|p| p.raw)
}

/// Parse one DSL script — one action per line, `#` comments.
///
/// Lines are processed in order; empty and `#`-prefixed lines are skipped.
/// Each action line is `VERB key=value ...`; unknown parameters are
/// ignored (forward-compatible scripts), unknown verbs and unparseable
/// numbers are errors. Prices follow one rule everywhere: a bare integer
/// is already-scaled ticks, any other text is parsed as a scale-4 decimal
/// (`10.50` → `105_000`).
///
/// # Examples
/// ```
/// use exchange8_sim_harness::{parse_script, Action};
///
/// let acts = parse_script("Z\nX trader=mm sym=ACME\n").unwrap();
/// assert!(matches!(acts[0], Action::Flush));
/// assert!(matches!(
///     acts[1],
///     Action::CancelAll { symbol: Some(_), .. }
/// ));
/// ```
///
/// # Errors
/// Returns [`ParseError`] with the 1-based line number: `BadVerb` for an
/// unknown verb, `MissingField` for an absent required parameter,
/// `BadNumber` for an unparseable numeric value. A failing line fails the
/// whole script — no partial action vector is returned.
pub fn parse_script(src: &str) -> Result<Vec<Action>, ParseError> {
    let mut out = Vec::new();
    for (idx, raw_line) in src.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let verb = parts.next().unwrap_or_default().to_string();
        let params: Vec<(&str, &str)> = parts
            .filter_map(|tok| {
                tok.split_once('=')
                    .map(|(k, v)| (k.trim(), v.trim()))
                    .filter(|(k, _)| !k.is_empty())
            })
            .collect();

        let action = match verb.as_str() {
            "D" | "F" => {
                let trader = getp(&params, "trader").ok_or(ParseError::MissingField {
                    line: line_no,
                    field: "trader",
                })?;
                let symbol = getp(&params, "sym").ok_or(ParseError::MissingField {
                    line: line_no,
                    field: "sym",
                })?;
                let side = getp(&params, "side").ok_or(ParseError::MissingField {
                    line: line_no,
                    field: "side",
                })?;
                let qty: u64 = getp(&params, "qty")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "qty",
                    })?
                    .parse()
                    .map_err(|_| ParseError::BadNumber {
                        line: line_no,
                        got: getp(&params, "qty").unwrap_or_default().into(),
                    })?;
                let price = getp(&params, "px").and_then(parse_px);
                let tif = match getp(&params, "tif") {
                    Some("ioc") => Tif::Ioc,
                    Some("fok") => Tif::Fok,
                    _ => Tif::Day,
                };
                let req = OrderRequest {
                    trader: trader.into(),
                    symbol: symbol.into(),
                    side: side.chars().next().unwrap_or('B'),
                    qty,
                    price,
                    tif,
                };
                if verb == "D" {
                    Action::Deposit(req)
                } else {
                    Action::Enter(req)
                }
            }
            "X" => {
                let trader = getp(&params, "trader")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "trader",
                    })?
                    .to_string();
                match getp(&params, "oid") {
                    Some(oid) => Action::Cancel {
                        trader,
                        order_id: oid.parse().map_err(|_| ParseError::BadNumber {
                            line: line_no,
                            got: oid.into(),
                        })?,
                    },
                    None => Action::CancelAll {
                        trader,
                        symbol: getp(&params, "sym").map(|s| s.to_string()),
                    },
                }
            }
            "Z" => Action::Flush,
            "R" => {
                let trader = getp(&params, "trader")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "trader",
                    })?
                    .to_string();
                let oid: u64 = getp(&params, "oid")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "oid",
                    })?
                    .parse()
                    .map_err(|_| ParseError::BadNumber {
                        line: line_no,
                        got: getp(&params, "oid").unwrap_or_default().into(),
                    })?;
                let symbol = getp(&params, "sym")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "sym",
                    })?
                    .to_string();
                let side = getp(&params, "side")
                    .and_then(|s| s.chars().next())
                    .unwrap_or('B');
                let qty: u64 = getp(&params, "qty")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "qty",
                    })?
                    .parse()
                    .map_err(|_| ParseError::BadNumber {
                        line: line_no,
                        got: getp(&params, "qty").unwrap_or_default().into(),
                    })?;
                let price = getp(&params, "px").and_then(parse_px);
                Action::Replace {
                    trader: trader.clone(),
                    order_id: oid,
                    new: Box::new(OrderRequest {
                        trader,
                        symbol,
                        side,
                        qty,
                        price,
                        tif: Tif::Day,
                    }),
                }
            }
            "E" => {
                let symbol = getp(&params, "sym")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "sym",
                    })?
                    .to_string();
                let px = getp(&params, "px").ok_or(ParseError::MissingField {
                    line: line_no,
                    field: "px",
                })?;
                Action::Print {
                    symbol,
                    price: parse_px(px).ok_or(ParseError::BadNumber {
                        line: line_no,
                        got: px.into(),
                    })?,
                    qty: getp(&params, "qty").unwrap_or("0").parse().map_err(|_| {
                        ParseError::BadNumber {
                            line: line_no,
                            got: getp(&params, "qty").unwrap_or_default().into(),
                        }
                    })?,
                }
            }
            "S" => {
                let symbol = getp(&params, "sym")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "sym",
                    })?
                    .to_string();
                let side = getp(&params, "side")
                    .and_then(|s| s.chars().next())
                    .unwrap_or('B');
                Action::Sweep {
                    symbol,
                    side,
                    to_px: getp(&params, "to").and_then(parse_px).ok_or(
                        ParseError::MissingField {
                            line: line_no,
                            field: "to",
                        },
                    )?,
                }
            }
            "2" => {
                let trader = getp(&params, "trader")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "trader",
                    })?
                    .to_string();
                let symbol = getp(&params, "sym")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "sym",
                    })?
                    .to_string();
                let buy = getp(&params, "bid")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "bid",
                    })?
                    .to_string();
                let sell = getp(&params, "ask")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "ask",
                    })?
                    .to_string();
                Action::TwoSided {
                    trader,
                    symbol,
                    buy_px: parse_px(&buy).ok_or(ParseError::BadNumber {
                        line: line_no,
                        got: buy.clone(),
                    })?,
                    sell_px: parse_px(&sell).ok_or(ParseError::BadNumber {
                        line: line_no,
                        got: sell,
                    })?,
                    qty: getp(&params, "qty")
                        .and_then(|q| q.parse().ok())
                        .unwrap_or(100),
                }
            }
            "Q" => {
                let symbol = getp(&params, "sym")
                    .ok_or(ParseError::MissingField {
                        line: line_no,
                        field: "sym",
                    })?
                    .to_string();
                Action::Quote {
                    symbol,
                    bid: getp(&params, "bid").and_then(parse_px),
                    ask: getp(&params, "ask").and_then(parse_px),
                }
            }
            "T" => Action::Flush, // tape print: no book effect in the harness
            other => {
                return Err(ParseError::BadVerb {
                    line: line_no,
                    verb: other.into(),
                });
            }
        };
        out.push(action);
    }
    Ok(out)
}

/// Map a DSL side char to `OrderSide`.
///
/// The DSL's spec-faithful sides are `'B'` (buy) and `'S'` (sell); any
/// other character falls through to buy — a deliberate spec-compat choice,
/// with validation left to the consuming engine.
///
/// # Examples
/// ```
/// use exchange8_contracts::OrderSide;
/// use exchange8_sim_harness::side_of;
///
/// assert_eq!(side_of('S'), OrderSide::Sell);
/// assert_eq!(side_of('B'), OrderSide::Buy);
/// ```
pub fn side_of(c: char) -> OrderSide {
    match c {
        'S' => OrderSide::Sell,
        _ => OrderSide::Buy,
    }
}

/// Map a DSL order to its engine order type.
///
/// Deposit (`D`) and enter (`F`) actions are limit orders in the harness —
/// the engine decides market semantics from `price: None` on the request.
/// The `Tif` parameter exists for signature stability with the
/// `OrderRequest` it accompanies; the mapping does not read it.
///
/// # Examples
/// ```
/// use exchange8_contracts::{OrdType, Tif};
/// use exchange8_sim_harness::order_type_of;
///
/// assert_eq!(order_type_of(Tif::Ioc), OrdType::Limit);
/// ```
pub fn order_type_of(tif: Tif) -> OrdType {
    let _ = tif;
    OrdType::Limit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_deposit_and_enter() {
        let script = "\
# a comment
D trader=alice sym=ACME side=B qty=100 px=10.50
F trader=bob sym=ACME side=S qty=100 px=10.50 tif=ioc
";
        let acts = parse_script(script).unwrap();
        assert_eq!(acts.len(), 2);
        match &acts[0] {
            Action::Deposit(r) => {
                assert_eq!(r.trader, "alice");
                assert_eq!(r.symbol, "ACME");
                assert_eq!(r.side, 'B');
                assert_eq!(r.qty, 100);
                assert_eq!(r.price, Some(105_000));
                assert_eq!(r.tif, Tif::Day);
            }
            other => panic!("expected Deposit, got {other:?}"),
        }
        match &acts[1] {
            Action::Enter(r) => {
                assert_eq!(r.tif, Tif::Ioc);
                assert_eq!(r.side, 'S');
            }
            other => panic!("expected Enter, got {other:?}"),
        }
    }

    #[test]
    fn parse_cancel_and_flush() {
        let script = "X trader=alice oid=42\nX trader=alice sym=ACME\nZ\n";
        let acts = parse_script(script).unwrap();
        assert_eq!(acts.len(), 3);
        assert!(matches!(&acts[0], Action::Cancel { order_id: 42, .. }));
        assert!(matches!(
            &acts[1],
            Action::CancelAll {
                symbol: Some(_),
                ..
            }
        ));
        assert!(matches!(&acts[2], Action::Flush));
    }

    #[test]
    fn parse_quotes_and_sweep() {
        let script = "Q sym=ACME bid=10.00 ask=10.10\nS sym=ACME side=B to=10.50\n2 trader=mm sym=ACME bid=10.00 ask=10.10 qty=200\n";
        let acts = parse_script(script).unwrap();
        assert_eq!(acts.len(), 3);
        assert!(matches!(
            &acts[0],
            Action::Quote {
                bid: Some(100_000),
                ask: Some(101_000),
                ..
            }
        ));
        assert!(matches!(&acts[1], Action::Sweep { side: 'B', .. }));
        assert!(matches!(&acts[2], Action::TwoSided { qty: 200, .. }));
    }

    #[test]
    fn bare_int_price_is_scaled() {
        let acts = parse_script("D trader=a sym=X side=B qty=1 px=100000").unwrap();
        match &acts[0] {
            Action::Deposit(r) => assert_eq!(r.price, Some(100_000)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bad_verb_rejected() {
        let err = parse_script("W trader=a").unwrap_err();
        assert!(matches!(err, ParseError::BadVerb { verb, .. } if verb == "W"));
    }

    #[test]
    fn missing_field_rejected() {
        let err = parse_script("D trader=a").unwrap_err();
        assert!(matches!(err, ParseError::MissingField { field: "sym", .. }));
    }
}
