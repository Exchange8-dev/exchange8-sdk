// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! Exchange8 fixed-point numeric core.
//!
//! Prices are `i128` scaled by a **per-value decimal exponent**: equities
//! default ×10⁴ (`EQUITY_SCALE`), FX ×10⁵ (`FX_SCALE`), crypto ×10⁸
//! (`CRYPTO_SCALE`). Quantities are decimal-scaled `u64` (integer board
//! lots for equities, fractional scale 8 for crypto). Arithmetic is
//! overflow-checked on `i128`; two-operand operations refuse scale
//! mismatch rather than mixing precisions.
//!
//! # Quick start
//!
//! ```rust
//! use exchange8_fixed::{Price, Qty, CRYPTO_SCALE};
//!
//! let px = Price::parse_decimal("65432.10", CRYPTO_SCALE).unwrap();
//! let qty = Qty::parse_decimal("0.5", CRYPTO_SCALE).unwrap();
//! let notional = px.checked_notional(qty).unwrap();
//! assert_eq!(notional, 3_271_605_000_000); // 32716.05 in price-tick units
//! ```
//!
//! # Overview
//!
//! - `Price` — scale-carrying fixed-point price with lossless rescale,
//!   checked arithmetic, and parse from decimal strings.
//! - `Qty` — scale-carrying quantity with checked operations and
//!   board-lot/odd-lot decomposition.
//! - Tick-rule functions — equity band ticks, doubled midpoint grid,
//!   FX pip and half-pip grids.
//!
//! Detailed design: [`DESIGN.md`](../../DESIGN.md); crate overview:
//! [`README.md`](../../README.md).

use serde::{Deserialize, Serialize};

/// Equity price scale: 4 decimal digits (×10⁴).
pub const EQUITY_SCALE: u8 = 4;
/// FX price scale: 5 decimals (pip-hundredths; EURUSD 1.08456).
pub const FX_SCALE: u8 = 5;
/// One pip at FX scale = 0.0001 = 10 raw units at scale 5.
pub const FX_PIP_TICKS: i128 = 10;
/// Crypto price scale: 8 decimal digits (×10⁸).
pub const CRYPTO_SCALE: u8 = 8;
/// Default quantity scale: integer quantities (board lots).
pub const QTY_SCALE_INT: u8 = 0;
/// Fractional quantity scale (crypto).
pub const QTY_SCALE_FRAC: u8 = 8;

/// 10ⁿ for u8 exponents (checked).
const fn pow10(exp: u8) -> i128 {
    let mut r: i128 = 1;
    let mut i = 0u8;
    while i < exp {
        r *= 10;
        i += 1;
    }
    r
}

/// A scaled price. The `scale` (decimal digits) is carried per value so an
/// instrument with 4 digits and one with 8 never mix silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Price {
    /// Scaled integer value (ticks).
    pub raw: i128,
    /// Decimal digits (4 = equity, 8 = crypto).
    pub scale: u8,
}

impl Price {
    /// Build from an already-scaled integer.
    pub const fn from_scaled(raw: i128, scale: u8) -> Self {
        Price { raw, scale }
    }

    /// Equity constructor (scale 4). `Price::equity(100_000)` == 10.00.
    pub const fn equity(raw: i128) -> Self {
        Price {
            raw,
            scale: EQUITY_SCALE,
        }
    }

    /// Parse a decimal string (`"10.05"`) at the given scale.
    ///
    /// Returns `None` on: more fractional digits than `scale`, non-digits,
    /// empty integer part, or a negative value (prices are positive).
    pub fn parse_decimal(s: &str, scale: u8) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let neg = s.starts_with('-');
        let body = if neg { &s[1..] } else { s };
        let (int_part, frac_part) = match body.split_once('.') {
            Some((i, f)) => (i, f),
            None => (body, ""),
        };
        if int_part.is_empty() {
            return None;
        }
        if !int_part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if frac_part.len() > scale as usize {
            return None;
        }
        if !frac_part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        // Digits beyond the scale were rejected above; pad the rest.
        let mut frac_val: i128 = 0;
        for b in frac_part.bytes() {
            frac_val = frac_val * 10 + (b - b'0') as i128;
        }
        for _ in frac_part.len()..scale as usize {
            frac_val *= 10;
        }
        let int_val: i128 = int_part.parse().ok()?;
        let raw = int_val * pow10(scale) + frac_val;
        // Exchange prices are strictly positive;
        // negatives are rejected outright.
        if neg || raw <= 0 {
            return None;
        }
        Some(Price { raw, scale })
    }

    /// Re-scale to a common exponent for comparison/arithmetic.
    /// Only upward re-scaling is lossless; returns `None` if it would truncate.
    pub fn rescale(self, to: u8) -> Option<Price> {
        if to == self.scale {
            return Some(self);
        }
        if to > self.scale {
            let mut r = self.raw;
            let mut i = self.scale;
            while i < to {
                r = r.checked_mul(10)?;
                i += 1;
            }
            Some(Price { raw: r, scale: to })
        } else {
            // Downscale only when it divides exactly.
            let div = pow10(self.scale - to);
            if self.raw % div == 0 {
                Some(Price {
                    raw: self.raw / div,
                    scale: to,
                })
            } else {
                None
            }
        }
    }

    /// Checked addition of two same-scale prices.
    pub fn checked_add(self, rhs: Price) -> Option<Price> {
        if self.scale != rhs.scale {
            return None;
        }
        self.raw.checked_add(rhs.raw).map(|r| Price {
            raw: r,
            scale: self.scale,
        })
    }

    /// Add `ticks` (1-tick units at this price's scale) with overflow check.
    pub fn checked_add_ticks(self, ticks: i128) -> Option<Price> {
        self.raw.checked_add(ticks).map(|r| Price {
            raw: r,
            scale: self.scale,
        })
    }

    /// Notional = price × qty, computed overflow-safely.
    ///
    /// **Semantics**: the result is expressed in the **price's** tick units —
    /// e.g. an equity price ×10⁴ times an integer qty yields the notional
    /// scaled ×10⁴ (1.00 × 100 = 100.00 → `1_000_000`). A fractional qty is
    /// applied as `price.raw × qty.raw / 10^qty.scale` (multiply first, then
    /// divide, to preserve precision).
    pub fn checked_notional(self, qty: Qty) -> Option<i128> {
        let product = self.raw.checked_mul(qty.raw as i128)?;
        if qty.scale == 0 {
            Some(product)
        } else {
            product.checked_div(pow10(qty.scale))
        }
    }
}

/// A decimal-scaled quantity. Equities: scale 0 (integer board lots);
/// crypto: scale 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Qty {
    /// Scaled integer value.
    pub raw: u64,
    /// Decimal digits.
    pub scale: u8,
}

impl Qty {
    /// Zero at a scale.
    pub const ZERO: Qty = Qty {
        raw: 0,
        scale: QTY_SCALE_INT,
    };

    /// Integer (board-lot) quantity.
    pub const fn units(raw: u64) -> Self {
        Qty {
            raw,
            scale: QTY_SCALE_INT,
        }
    }

    /// Raw value at a scale.
    pub const fn from_scaled(raw: u64, scale: u8) -> Self {
        Qty { raw, scale }
    }

    /// Checked subtraction (`None` when rhs > self).
    pub fn checked_sub(self, rhs: Qty) -> Option<Qty> {
        if self.scale != rhs.scale {
            return None;
        }
        self.raw.checked_sub(rhs.raw).map(|r| Qty {
            raw: r,
            scale: self.scale,
        })
    }

    /// Saturating addition.
    pub fn saturating_add(self, rhs: Qty) -> Qty {
        if self.scale != rhs.scale {
            return self;
        }
        Qty {
            raw: self.raw.saturating_add(rhs.raw),
            scale: self.scale,
        }
    }

    /// Parse a decimal string at the given scale.
    pub fn parse_decimal(s: &str, scale: u8) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let (int_part, frac_part) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };
        if int_part.is_empty() || frac_part.len() > scale as usize {
            return None;
        }
        if !int_part.bytes().all(|b| b.is_ascii_digit())
            || !frac_part.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        // Fractional digits, right-padded to the scale.
        let mut frac_val: u64 = 0;
        let mut width = frac_part.len();
        for b in frac_part.bytes() {
            frac_val = frac_val * 10 + (b - b'0') as u64;
        }
        while width < scale as usize {
            frac_val *= 10;
            width += 1;
        }
        // Int part scaled by 10^scale, plus the fractional value.
        let int_val: u64 = int_part.parse().ok()?;
        let scaled = int_val
            .checked_mul(pow10(scale) as u64)?
            .checked_add(frac_val)?;
        Some(Qty { raw: scaled, scale })
    }

    /// Split a quantity into (whole-lot count, odd-lot volume).
    ///
    /// **Semantic care**: returns the *count* of whole lots first and the
    /// odd-lot *volume* second — multiply the count by lot size to get board
    /// volume (mixed-lot split convention; see `DESIGN.md` §4).
    /// Only meaningful at scale 0 (integer quantities); a fractional qty is
    /// all-odd.
    pub fn split_board_odd(self, lot: Qty) -> (Qty, Qty) {
        if self.scale != 0 || lot.scale != 0 || lot.raw == 0 {
            return (Qty::units(0), self);
        }
        let lots = self.raw / lot.raw;
        let odd = self.raw % lot.raw;
        (Qty::units(lots), Qty::units(odd))
    }
}

/// Tick-size lookup: price ≥ 0.50 → 1¢ (100 ticks), else 0.5¢
/// (50 ticks) at equity scale 4.
pub fn equity_tick(price: Price) -> i128 {
    debug_assert_eq!(price.scale, EQUITY_SCALE);
    if price.raw >= pow10(EQUITY_SCALE) / 2 {
        100
    } else {
        50
    }
}

/// True when `price` is a multiple of the equity tick for its band.
pub fn on_equity_tick(price: Price) -> bool {
    let tick = equity_tick(price);
    price.raw % tick == 0
}

/// Doubled-tick validation for midpoint/dark peg prices (OMS
/// `tick_size()` ×2 — a midpoint can land on a half-tick).
pub fn on_doubled_equity_tick(price: Price) -> bool {
    let tick = equity_tick(price) * 2;
    price.raw % tick == 0
}

/// FX pip tick rule: every price must be a whole pip (0.0001) at scale 5.
/// Most pairs quote to the pip; fifth-digit (fractional pip / pipette)
/// quotes are venue-configurable and would use `on_fx_half_pip_tick`.
pub fn on_fx_pip_tick(price: Price) -> bool {
    debug_assert_eq!(price.scale, FX_SCALE);
    price.raw % FX_PIP_TICKS == 0
}

/// FX pipette rule: half-pip increments (0.00005) allowed — used by
/// venues that stream fifth-digit prices.
pub fn on_fx_half_pip_tick(price: Price) -> bool {
    debug_assert_eq!(price.scale, FX_SCALE);
    price.raw % (FX_PIP_TICKS / 2) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fx_decimal() {
        // EURUSD 1.08456 at scale 5 (pipette precision).
        let p = Price::parse_decimal("1.08456", FX_SCALE).unwrap();
        assert_eq!(p.raw, 108_456);
        assert_eq!(p.scale, FX_SCALE);
        // Whole-pip quote 1.0845 parses too.
        let pip = Price::parse_decimal("1.0845", FX_SCALE).unwrap();
        assert_eq!(pip.raw, 108_450);
        // Six fractional digits rejected at scale 5.
        assert_eq!(Price::parse_decimal("1.084567", FX_SCALE), None);
    }

    #[test]
    fn fx_pip_tick_rules() {
        // 1.08450 — whole pip (raw 108450 % 10 == 0).
        let pip = Price::from_scaled(108_450, FX_SCALE);
        assert!(on_fx_pip_tick(pip));
        assert!(on_fx_half_pip_tick(pip));
        // 1.08455 — a pipette: valid half-pip, NOT a whole pip.
        let pipette = Price::from_scaled(108_455, FX_SCALE);
        assert!(!on_fx_pip_tick(pipette));
        assert!(on_fx_half_pip_tick(pipette));
        // 1.08452 — sub-half-pip: invalid under both rules.
        let odd = Price::from_scaled(108_452, FX_SCALE);
        assert!(!on_fx_pip_tick(odd));
        assert!(!on_fx_half_pip_tick(odd));
        // Rescale pip ↔ pipette is lossless both ways.
        assert_eq!(pip.rescale(6).unwrap().raw, 1_084_500);
        assert_eq!(pipette.rescale(6).unwrap().raw, 1_084_550);
    }

    #[test]
    fn parse_crypto_decimal() {
        let p = Price::parse_decimal("65432.12345678", CRYPTO_SCALE).unwrap();
        assert_eq!(p.raw, 6_543_212_345_678);
        assert_eq!(p.scale, 8);
        // 9 fractional digits rejected
        assert_eq!(Price::parse_decimal("0.000000001", CRYPTO_SCALE), None);
    }

    #[test]
    fn rescale_up_lossless() {
        let p = Price::equity(100_500);
        let up = p.rescale(8).unwrap();
        assert_eq!(up.raw, 1_005_000_000);
        // Downscale back exact
        assert_eq!(up.rescale(4).unwrap(), p);
        // Downscale with truncation rejected
        let bad = Price::from_scaled(1, 8);
        assert_eq!(bad.rescale(4), None);
    }

    #[test]
    fn equity_tick_bands() {
        // ≥ $0.50 → 100 (1¢); below → 50 (0.5¢)
        assert_eq!(equity_tick(Price::equity(5_000)), 100);
        assert_eq!(equity_tick(Price::equity(4_999)), 50);
        assert!(on_equity_tick(Price::equity(10_100)));
        assert!(!on_equity_tick(Price::equity(10_050)));
        // Doubled tick (200): only multiples of 200 pass. 10_050 and 10_100
        // are on the single-tick grid (×100) but not ×200; 10_200 is.
        assert!(!on_doubled_equity_tick(Price::equity(10_050)));
        assert!(!on_doubled_equity_tick(Price::equity(10_100)));
        assert!(on_doubled_equity_tick(Price::equity(10_200)));
    }

    #[test]
    fn split_board_odd() {
        // lot 100, qty 350 → 3 lots, 50 odd
        let (lots, odd) = Qty::units(350).split_board_odd(Qty::units(100));
        assert_eq!((lots.raw, odd.raw), (3, 50));
        // exact board
        let (lots, odd) = Qty::units(300).split_board_odd(Qty::units(100));
        assert_eq!((lots.raw, odd.raw), (3, 0));
        // pure odd
        let (lots, odd) = Qty::units(50).split_board_odd(Qty::units(100));
        assert_eq!((lots.raw, odd.raw), (0, 50));
        // fractional (crypto) is all-odd
        let q = Qty::parse_decimal("0.001", 8).unwrap();
        let (lots, odd) = q.split_board_odd(Qty::units(1));
        assert_eq!(lots.raw, 0);
        assert_eq!(odd.raw, q.raw);
    }

    #[test]
    fn notional_checked() {
        let px = Price::equity(10_000); // 1.00
        let qty = Qty::units(100);
        // raw product: 1.00 × 100 = 100.00 in price ticks
        assert_eq!(px.checked_notional(qty).unwrap(), 1_000_000);
        // Crypto cross-scale: 65000.00 × 0.5 = 32500.00 → raw ×10⁸ = 3.25e12
        let cpx = Price::parse_decimal("65000.00", 8).unwrap();
        let cq = Qty::parse_decimal("0.5", 8).unwrap();
        assert_eq!(cpx.raw, 6_500_000_000_000);
        assert_eq!(cq.raw, 50_000_000);
        assert_eq!(cpx.checked_notional(cq).unwrap(), 3_250_000_000_000);
        // Overflow
        let big = Price::from_scaled(i128::MAX / 2, 4);
        assert!(big.checked_notional(Qty::units(u64::MAX)).is_none());
    }

    #[test]
    fn qty_parse() {
        assert_eq!(Qty::parse_decimal("100", 0).unwrap().raw, 100);
        let q = Qty::parse_decimal("1.5", 8).unwrap();
        assert_eq!(q.raw, 150_000_000);
        assert_eq!(Qty::parse_decimal("1.123456789", 8), None);
        // Exactly-scale digits valid
        assert_eq!(Qty::parse_decimal("0.00000001", 8).unwrap().raw, 1);
    }
}
