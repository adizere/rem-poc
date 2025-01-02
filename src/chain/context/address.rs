use std::fmt;

use malachite_core_types::Address;

/// The simplest representation of an address in the network.
/// Implements [`Address`].
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd, Debug, Copy, Hash)]
pub struct BasePeerAddress(pub u32);

impl BasePeerAddress {
    pub fn new(address: u32) -> Self {
        Self(address)
    }
}

impl Address for BasePeerAddress {}

impl fmt::Display for BasePeerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "peer {}", self.0)
    }
}

impl From<usize> for BasePeerAddress {
    fn from(value: usize) -> Self {
        Self(value as u32)
    }
}
