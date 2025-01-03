use std::fmt;

use malachite_core_types::Round;

use crate::chain::context::address::BasePeerAddress;
use crate::chain::context::height::BaseHeight;
use crate::chain::context::value::BaseValue;
use crate::chain::context::BaseContext;

/// A proposal for a value
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseProposal {
    pub height: BaseHeight,
    pub value: BaseValue,
    pub proposer: BasePeerAddress,
    pub round: Round,
    // Todo: Clarify if needs to be exposed here or at lower levels
    //  of abstraction?
    // pub pol_round: Round,
}

impl BaseProposal {
    // Todo: We should be marshaling to bytes all fields here not just the value
    //  This is only used for signing logic, so it's fine in a proof-of-concept
    pub fn to_bytes(&self) -> [u8; size_of::<u64>()] {
        // Serialize just the value identifier, a u64
        self.value.id.to_be_bytes()
    }
}

impl fmt::Display for BaseProposal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Proposal / {} / {} / {:?} / <- {}",
            self.height, self.round, self.value, self.proposer
        )
    }
}

impl malachite_core_types::Proposal<BaseContext> for BaseProposal {
    fn height(&self) -> BaseHeight {
        self.height
    }

    fn round(&self) -> Round {
        self.round
    }

    // Todo: How come value() returns a Ctx::Value
    //  instead of returning a Ctx::ValueId ?
    fn value(&self) -> &BaseValue {
        &self.value
    }

    // Todo: This seems un-necessary.
    fn take_value(self) -> BaseValue {
        self.value
    }

    // Todo: Seems like exactly the kind of stuff we can
    //  abstract in a "base" layer of primitives ?
    fn pol_round(&self) -> Round {
        // We assume we never need to go into round > 0
        Round::Nil
    }

    fn validator_address(&self) -> &BasePeerAddress {
        &self.proposer
    }
}

/// A part of a proposal
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseProposalPart {
    pub height: BaseHeight,
    // No need for anything else, for the moment
}

#[allow(unused)]
impl BaseProposalPart {
    pub fn new(h: BaseHeight) -> Self {
        Self { height: h }
    }
}

impl malachite_core_types::ProposalPart<BaseContext> for BaseProposalPart {
    fn is_first(&self) -> bool {
        true
    }

    fn is_last(&self) -> bool {
        // Todo: Why is this needed?
        //  Maybe just needed in case of streaming proposals?
        unimplemented!()
    }
}
