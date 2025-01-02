use crate::chain::context::address::BasePeerAddress;
use crate::chain::context::height::BaseHeight;
use crate::chain::context::value::BaseValueId;

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
