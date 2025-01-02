use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Ord, PartialOrd)]
pub struct BaseValue(pub u64);

// Todo: Is this overkill?
//  Seems necessary for fulfilling Vote::value().
#[derive(Copy, Clone, PartialEq, Eq, Debug, Ord, PartialOrd)]
pub struct BaseValueId(pub u64);

impl malachite_core_types::Value for BaseValue {
    type Id = BaseValueId;

    fn id(&self) -> Self::Id {
        BaseValueId(self.0)
    }
}

impl fmt::Display for BaseValueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
