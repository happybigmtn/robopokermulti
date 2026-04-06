//! 3-Player Kuhn Poker serves as a minimal multiway toy example for testing multiway CFR.
//!
//! This implements a simplified 3-player Kuhn poker variant:
//! - 3 cards: Jack (0), Queen (1), King (2), one dealt to each player
//! - Single betting round: bet 1 chip or check/call/fold
//! - Higher card wins the pot
//!
//! This validates that multiway MCCFR converges correctly by:
//! - Testing walker rotation across 3 players
//! - Verifying regret accumulation for multiple opponents
//! - Checking that strategies converge to known equilibrium bounds

mod edge;
mod game;
mod solver;
mod turn;

pub use edge::*;
pub use game::*;
pub use solver::*;
pub use turn::*;
