// Plugged from Malachite code base v0.0.1
// code/crates/signing-ed25519/src/lib.rs
// Todo: Use an off-the-shelf option

use rand::{CryptoRng, RngCore};
use signature::{Keypair, Signer, Verifier};

use malachite_core_types::SigningScheme;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Ed25519;

impl Ed25519 {
    pub fn generate_keypair<R>(rng: R) -> PrivateKey
    where
        R: RngCore + CryptoRng,
    {
        PrivateKey::generate(rng)
    }
}

impl SigningScheme for Ed25519 {
    type DecodingError = ed25519_consensus::Error;

    type Signature = Signature;
    type PublicKey = PublicKey;
    type PrivateKey = PrivateKey;

    fn decode_signature(bytes: &[u8]) -> Result<Self::Signature, Self::DecodingError> {
        Signature::try_from(bytes)
    }

    fn encode_signature(signature: &Signature) -> Vec<u8> {
        signature.as_bytes().to_vec()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Signature(ed25519_consensus::Signature);

impl Signature {
    pub fn inner(&self) -> &ed25519_consensus::Signature {
        &self.0
    }

    pub fn as_bytes(&self) -> [u8; 64] {
        self.0.to_bytes()
    }
}

impl From<ed25519_consensus::Signature> for Signature {
    fn from(signature: ed25519_consensus::Signature) -> Self {
        Self(signature)
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = ed25519_consensus::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self(ed25519_consensus::Signature::try_from(bytes)?))
    }
}

impl PartialOrd for Signature {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Signature {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.to_bytes().cmp(&other.0.to_bytes())
    }
}

#[derive(Clone, Debug)]
pub struct PrivateKey(ed25519_consensus::SigningKey);

impl PrivateKey {
    pub fn generate<R>(rng: R) -> Self
    where
        R: RngCore + CryptoRng,
    {
        let signing_key = ed25519_consensus::SigningKey::new(rng);

        Self(signing_key)
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey::new(self.0.verification_key())
    }

    pub fn sign(&self, msg: &[u8]) -> Signature {
        Signature(self.0.sign(msg))
    }
}

impl From<[u8; 32]> for PrivateKey {
    fn from(bytes: [u8; 32]) -> Self {
        Self(ed25519_consensus::SigningKey::from(bytes))
    }
}

impl Signer<Signature> for PrivateKey {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        Ok(Signature(self.0.sign(msg)))
    }
}

impl Keypair for PrivateKey {
    type VerifyingKey = PublicKey;

    fn verifying_key(&self) -> Self::VerifyingKey {
        self.public_key()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(ed25519_consensus::VerificationKey);

impl PublicKey {
    pub fn new(key: impl Into<ed25519_consensus::VerificationKey>) -> Self {
        Self(key.into())
    }

    pub fn verify(&self, msg: &[u8], signature: &Signature) -> Result<(), signature::Error> {
        self.0
            .verify(signature.inner(), msg)
            .map_err(|_| signature::Error::new())
    }
}

impl Verifier<Signature> for PublicKey {
    fn verify(&self, msg: &[u8], signature: &Signature) -> Result<(), signature::Error> {
        PublicKey::verify(self, msg, signature)
    }
}
