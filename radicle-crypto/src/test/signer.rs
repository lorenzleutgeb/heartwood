use crate::{KeyPair, PublicKey, SecretKey, Seed, Signature, Signer, SignerError};

#[derive(Debug, Clone)]
pub struct MockSigner {
    pub(crate) pk: PublicKey,
    pub(crate) sk: SecretKey,
}

impl MockSigner {
    pub fn new(rng: &mut fastrand::Rng) -> Self {
        let mut seed: [u8; 32] = [0; 32];

        for byte in &mut seed {
            *byte = rng.u8(..);
        }
        Self::from_seed(seed)
    }

    pub fn from_seed(seed: [u8; 32]) -> Self {
        let seed = Seed::new(seed);
        let keypair = KeyPair::from_seed(seed);

        Self::from(SecretKey::from(keypair.sk))
    }
}

impl From<SecretKey> for MockSigner {
    fn from(sk: SecretKey) -> Self {
        let pk = sk.public_key().into();
        Self { sk, pk }
    }
}

impl Default for MockSigner {
    fn default() -> Self {
        let seed = Seed::generate();
        let keypair = KeyPair::from_seed(seed);
        let sk = keypair.sk;

        Self {
            pk: sk.public_key().into(),
            sk: sk.into(),
        }
    }
}

impl PartialEq for MockSigner {
    fn eq(&self, other: &Self) -> bool {
        self.pk == other.pk
    }
}

impl Eq for MockSigner {}

impl std::hash::Hash for MockSigner {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.pk.hash(state)
    }
}

impl Signer for MockSigner {
    fn public_key(&self) -> &PublicKey {
        &self.pk
    }

    fn sign(&self, msg: &[u8]) -> Signature {
        self.sk.sign(msg, None).into()
    }

    fn try_sign(&self, msg: &[u8]) -> Result<Signature, SignerError> {
        Ok(self.sign(msg))
    }
}

impl signature::Signer<Signature> for MockSigner {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        Ok(self.sk.sign(msg, None).into())
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::EcSk for MockSigner {
    type Pk = PublicKey;

    // TODO: Should be renamed to 'generate'.
    fn generate_keypair() -> (Self, Self::Pk)
    where
        Self: Sized,
    {
        let kp = Self::default();
        let pk = kp.pk;

        (kp, pk)
    }

    fn to_pk(&self) -> Result<Self::Pk, cyphernet::EcSkInvalid> {
        Ok(self.pk)
    }
}

#[cfg(feature = "cyphernet")]
impl cyphernet::Ecdh for MockSigner {
    type SharedSecret = [u8; 32];

    fn ecdh(&self, pk: &Self::Pk) -> Result<Self::SharedSecret, cyphernet::EcdhError> {
        self.sk.ecdh(pk).map_err(|_| cyphernet::EcdhError::WeakPk)
    }
}
