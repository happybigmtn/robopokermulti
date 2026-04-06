//! Edge types for 3-player Kuhn poker actions.

use crate::mccfr::TreeEdge;
use crate::transport::Support;

/// Actions in 3-player Kuhn poker.
/// - Check: pass without betting (0)
/// - Bet: add 1 chip to pot (1)
/// - Call: match the current bet (2)
/// - Fold: surrender the pot (3)
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kuhn3Edge {
    Check,
    Bet,
    Call,
    Fold,
}

impl Support for Kuhn3Edge {}
impl TreeEdge for Kuhn3Edge {}

impl Kuhn3Edge {
    /// All possible edges (for iteration)
    pub const ALL: [Self; 4] = [Self::Check, Self::Bet, Self::Call, Self::Fold];
}
