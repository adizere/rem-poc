use std::fmt;

use malachite_core_types::{PublicKey, Validator, VotingPower};
use tracing::debug;

use crate::chain::context::address::BasePeerAddress;
use crate::chain::context::BaseContext;

/// This is the voting power of each peer.
pub const BASE_VOTING_POWER: u64 = 1;

/// The most basic definition of a peer.
/// All peers have equal voting power, [`BASE_VOTING_POWER`].
/// Implements [`Validator`] trait.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasePeer {
    pub id: BasePeerAddress,
    pub public_key: PublicKey<BaseContext>,
}

impl BasePeer {
    pub fn new(id: u32, public_key: PublicKey<BaseContext>) -> BasePeer {
        debug!(public_key = ?public_key, "created new peer");

        BasePeer {
            id: BasePeerAddress::new(id),
            public_key,
        }
    }
}

impl fmt::Display for BasePeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "peer {}", self.id)
    }
}

impl Validator<BaseContext> for BasePeer {
    fn address(&self) -> &BasePeerAddress {
        &self.id
    }

    fn public_key(&self) -> &PublicKey<BaseContext> {
        &self.public_key
    }

    fn voting_power(&self) -> VotingPower {
        BASE_VOTING_POWER
    }
}
