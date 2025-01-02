use malachite_core_types::{NilOrVal, Round, SignedExtension, VoteType};
use std::fmt;

use crate::chain::context::address::BasePeerAddress;
use crate::chain::context::height::BaseHeight;
use crate::chain::context::value::BaseValueId;
use crate::chain::context::BaseContext;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BaseVote {
    pub vote_type: VoteType,
    pub height: BaseHeight,
    pub value_id: NilOrVal<BaseValueId>,
    pub round: Round,
    pub voter: BasePeerAddress,
    pub extension: Option<SignedExtension<BaseContext>>,
}

impl BaseVote {
    // TODO: Similar to how we do it for `BaseProposal`, serialize only
    //  the height here as a quick prototype.
    // This is fine because we use `to_bytes`` only to get the payload to sign, not for
    // networking payloads.
    pub fn to_bytes(&self) -> [u8; size_of::<u64>()] {
        self.height.0.to_be_bytes()
    }
}

impl fmt::Display for BaseVote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} / {} / {} / {:?} / {} X[{:?}]",
            self.vote_type, self.height, self.round, self.value_id, self.voter, self.extension
        )
    }
}

impl malachite_core_types::Vote<BaseContext> for BaseVote {
    fn height(&self) -> BaseHeight {
        self.height
    }

    fn round(&self) -> Round {
        self.round
    }

    fn value(&self) -> &NilOrVal<BaseValueId> {
        &self.value_id
    }

    // Todo: Why is this needed?
    //  Candidate for deletion?
    fn take_value(self) -> NilOrVal<BaseValueId> {
        self.value_id
    }

    fn vote_type(&self) -> VoteType {
        self.vote_type
    }

    fn validator_address(&self) -> &BasePeerAddress {
        &self.voter
    }

    fn extension(&self) -> Option<&SignedExtension<BaseContext>> {
        self.extension.as_ref()
    }

    fn extend(self, extension: SignedExtension<BaseContext>) -> Self {
        Self {
            extension: Some(extension),
            ..self
        }
    }
}
