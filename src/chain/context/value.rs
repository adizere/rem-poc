use reth_primitives::TransactionSigned;
use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Debug)]
pub struct BaseValue {
    pub id: u64,
    pub transactions: Vec<TransactionSigned>,
}

impl malachite_core_types::Value for BaseValue {
    type Id = BaseValueId;

    fn id(&self) -> Self::Id {
        BaseValueId(self.id)
    }
}

impl PartialEq<Self> for BaseValue {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl PartialOrd<Self> for BaseValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl Eq for BaseValue {}

impl Ord for BaseValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Ord, PartialOrd)]
pub struct BaseValueId(pub u64);

impl fmt::Display for BaseValueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
