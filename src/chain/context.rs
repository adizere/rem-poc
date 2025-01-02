use malachite_core_types::{Context, NilOrVal, Round, ValueId, VoteType};

use address::BasePeerAddress;
use height::BaseHeight;
use peer::BasePeer;
use peer_set::BasePeerSet;
use proposals::{BaseProposal, BaseProposalPart};
use signing_provider::BaseSigningProvider;
use signing_scheme::PublicKey;
use value::BaseValue;
use vote::BaseVote;

// Type definitions needed for the context
pub mod address;
pub mod height;
pub mod peer;
pub mod peer_set;
pub mod proposals;
pub mod signing_provider;
pub mod signing_scheme;
pub mod value;
pub mod vote;

/// The context is shared across all the peers in the simulated network.
/// The signing provider within the context is also shared across all peers.
/// This means all peers have the same signing key: big assumption, but
/// inconsequential for the simulated environment here.
#[derive(Clone)]
pub struct BaseContext {
    pub signing_provider: BaseSigningProvider,
}

impl BaseContext {
    pub fn new() -> BaseContext {
        BaseContext {
            signing_provider: BaseSigningProvider::new(),
        }
    }

    // Get the public key that all peers share.
    pub fn shared_public_key(&self) -> PublicKey {
        self.signing_provider.public_key()
    }
}

impl Context for BaseContext {
    type Address = BasePeerAddress;
    type Height = BaseHeight;
    type ProposalPart = BaseProposalPart;
    type Proposal = BaseProposal;
    type Validator = BasePeer;
    type ValidatorSet = BasePeerSet;
    type Value = BaseValue;
    type Vote = BaseVote;
    type SigningScheme = signing_scheme::Ed25519;
    type SigningProvider = BaseSigningProvider;

    fn select_proposer<'a>(
        &self,
        validator_set: &'a Self::ValidatorSet,
        _height: Self::Height,
        _round: Round,
    ) -> &'a Self::Validator {
        // Keep it simple, the proposer is always the same peer
        validator_set
            .peers
            .first()
            .expect("no peer found in the validator set")
    }

    fn signing_provider(&self) -> &Self::SigningProvider {
        &self.signing_provider
    }

    fn new_proposal(
        height: Self::Height,
        round: Round,
        value: Self::Value,
        _pol_round: Round,
        address: Self::Address,
    ) -> Self::Proposal {
        BaseProposal {
            height,
            value,
            proposer: address,
            round,
        }
    }

    fn new_prevote(
        height: Self::Height,
        round: Round,
        value_id: NilOrVal<ValueId<Self>>,
        address: Self::Address,
    ) -> Self::Vote {
        BaseVote {
            vote_type: VoteType::Prevote,
            height,
            value_id,
            round,
            voter: address,
            // TODO: A bit strange there is option to put extension into Prevotes
            //  clarify.
            extension: None,
        }
    }

    fn new_precommit(
        height: Self::Height,
        round: Round,
        value_id: NilOrVal<ValueId<Self>>,
        address: Self::Address,
    ) -> Self::Vote {
        BaseVote {
            vote_type: VoteType::Precommit,
            height,
            value_id,
            round,
            voter: address,
            extension: None,
        }
    }
}
