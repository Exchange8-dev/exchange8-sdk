// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Exchange8:SDK
// SPDX-License-Identifier: Apache-2.0

//! # exchange8 — exchange8 workspace namespace facade
//!
//! Every exchange8 crate is named `exchange8::{leaf}` and is imported through this
//! facade. See `sentin8/design/sentin8-architecture-principles.md` §2.1.
//!
//! Not re-exported: `exchange8-sim-harness` (a dev/simulation tool whose
//! tests #[path]-include service modules — a facade re-export would create
//! a Cargo package cycle). Sim-harness consumers depend on it directly.

pub use exchange8_book as book;
pub use exchange8_contracts as contracts;
pub use exchange8_fix as fix;
pub use exchange8_fixed as fixed;
pub use exchange8_itch as itch;
pub use exchange8_mold as mold;
pub use exchange8_soup as soup;
pub use exchange8_stamp as stamp;
pub use exchange8_wire as wire;

pub mod me {
    pub use exchange8_me_client as client;
}
