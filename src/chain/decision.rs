use crate::chain::context::address::BasePeerAddress;
use crate::chain::context::height::BaseHeight;
use crate::chain::context::value::BaseValueId;
use crate::chain::context::BaseContext;
use malachite_core_consensus::ProposedValue;

/// Represents a step in the decision process that the
/// application can announce to the environment.
#[derive(Debug)]
pub enum DecisionStep {
    Proposed(ProposedValue<BaseContext>),
    Finalized(Decision),
}

/// Represents the finalized value that a certain peer reached
/// for a certain height [`BaseHeight`].
///
/// The full value is not captured here, merely the
/// identifier of that value: [`BaseValueId`].
#[derive(Debug)]
pub struct Decision {
    pub peer: BasePeerAddress,
    pub value_id: BaseValueId,
    pub height: BaseHeight,
}
