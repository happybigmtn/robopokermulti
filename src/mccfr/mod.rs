//! This module implements Counterfactual Regret Minimization (CFR) algorithms for various games.
//!
//! # Submodules
//!
//! - `nlhe`: Implementation of CFR for No-Limit Texas Hold'em poker
//! - `rps`: Simple Rock-Paper-Scissors implementation used as a toy example and test case
//! - `kuhn3`: 3-player Kuhn poker for validating multiway MCCFR (AC-2.3)
//! - `structs`: Core data structures used in CFR implementations
//! - `traits`: Generic traits that can be implemented for any tree-based game
//! - `types`: Type aliases and common types used across CFR implementations
//!
//! The module provides both concrete game implementations (`nlhe`, `rps`, `kuhn3`) as well as
//! generic infrastructure (`structs`, `traits`, `types`) that can be reused for
//! implementing CFR on any extensive-form game with perfect recall.
//!

pub mod cache;
pub mod kuhn3;
mod nlhe;
mod rps;
mod structs;
mod traits;
mod types;

pub use kuhn3::*;
pub use nlhe::*;
pub use rps::*;
pub use structs::*;
pub use traits::*;
pub use types::*;
