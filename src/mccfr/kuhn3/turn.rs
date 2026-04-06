//! Turn types for 3-player Kuhn poker.

use crate::mccfr::TreeTurn;

/// Turn indicator for 3-player Kuhn poker.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kuhn3Turn {
    /// Player 1's turn
    P1,
    /// Player 2's turn
    P2,
    /// Player 3's turn
    P3,
    /// Terminal state (showdown)
    Terminal,
}

impl Kuhn3Turn {
    /// Get the player index (0, 1, or 2)
    pub fn index(&self) -> usize {
        match self {
            Self::P1 => 0,
            Self::P2 => 1,
            Self::P3 => 2,
            Self::Terminal => panic!("terminal has no index"),
        }
    }
}

impl TreeTurn for Kuhn3Turn {
    fn chance() -> Self {
        // No chance nodes in this simplified variant
        // (we assume cards are dealt before the game tree starts)
        Self::Terminal
    }
}
