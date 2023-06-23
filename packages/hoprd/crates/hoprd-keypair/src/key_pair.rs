use crate::{
    errors::{KeyPairError, Result},
    keystore::{CipherparamsJson, CryptoJson, EthKeystore, KdfType, KdfparamsType, PrivateKeys},
};
use aes::{
    cipher::{self, InnerIvInit, KeyInit, StreamCipherCore},
    Aes128,
};
use core_crypto::types::{OffchainPublicKey, PublicKey};
use getrandom::getrandom;
use hex;
use scrypt::{scrypt, Params as ScryptParams};
use serde::{ser::SerializeStruct, Serialize, Serializer};
use serde_json::{from_str as from_json_string, to_string as to_json_string};
use sha3::{digest::Update, Digest, Keccak256};
use std::fmt::Debug;
use utils_log::error;
use utils_types::traits::{PeerIdLike, ToHex};
use uuid::Uuid;

#[cfg(all(feature = "wasm", not(test)))]
use real_base::file::wasm::{metadata, read_to_string, write};

#[cfg(any(not(feature = "wasm"), test))]
use real_base::file::native::{metadata, read_to_string, write};

const HOPR_CIPHER: &str = "aes-128-ctr";
const HOPR_KEY_SIZE: usize = 32usize;
const HOPR_IV_SIZE: usize = 16usize;
const HOPR_KDF_PARAMS_DKLEN: u8 = 32u8;
const HOPR_KDF_PARAMS_LOG_N: u8 = 13u8;
const HOPR_KDF_PARAMS_R: u32 = 8u32;
const HOPR_KDF_PARAMS_P: u32 = 1u32;

const PACKET_KEY_LENGTH: usize = 32;
const CHAIN_KEY_LENGTH: usize = 32;

pub type PacketKey = [u8; PACKET_KEY_LENGTH];
pub type ChainKey = [u8; CHAIN_KEY_LENGTH];

// Current version, deviates from pre 2.0
const VERSION: u32 = 2;

struct Aes128Ctr {
    inner: ctr::CtrCore<Aes128, ctr::flavors::Ctr128BE>,
}

impl Aes128Ctr {
    fn new(key: &[u8], iv: &[u8]) -> std::result::Result<Self, cipher::InvalidLength> {
        let cipher = aes::Aes128::new_from_slice(key).unwrap();
        let inner = ctr::CtrCore::inner_iv_slice_init(cipher, iv).unwrap();
        Ok(Self { inner })
    }

    fn apply_keystream(self, buf: &mut [u8]) {
        self.inner.apply_keystream_partial(buf.into());
    }
}

#[cfg_attr(feature = "wasm", wasm_bindgen::prelude::wasm_bindgen(getter_with_clone))]
pub struct IdentityOptions {
    pub initialize: bool,
    pub id_path: String,
    pub password: String,
    pub use_weak_crypto: Option<bool>,
    pub private_key: Option<Box<[u8]>>,
}

#[cfg_attr(feature = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
impl IdentityOptions {
    #[cfg_attr(feature = "wasm", wasm_bindgen::prelude::wasm_bindgen(constructor))]
    pub fn new(
        initialize: bool,
        id_path: String,
        password: String,
        use_weak_crypto: Option<bool>,
        private_key: Option<Box<[u8]>>,
    ) -> Self {
        Self {
            initialize,
            id_path,
            password,
            use_weak_crypto,
            private_key,
        }
    }
}

pub struct HoprKeys {
    pub packet_key: (PacketKey, OffchainPublicKey),
    pub chain_key: (ChainKey, PublicKey),
    pub id: Uuid,
}

impl Serialize for HoprKeys {
    /// Serialize without private keys
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("HoprKeys", 3)?;
        s.serialize_field("packet_key", self.packet_key.1.to_peerid_str().as_str())?;
        s.serialize_field("chain_key", &self.chain_key.1.to_peerid_str().as_str())?;
        s.serialize_field("uuid", &self.id)?;
        s.end()
    }
}

impl std::fmt::Display for HoprKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "packet_key: {}, chain_key: {} (Ethereum address: {})\nUUID: {}",
            self.packet_key.1.to_peerid_str(),
            self.chain_key.1.to_peerid_str(),
            self.chain_key.1.to_address(),
            self.id.to_string()
        )
    }
}

impl TryFrom<&str> for HoprKeys {
    type Error = KeyPairError;

    /// Deserializes HoprKeys from string
    ///
    /// ```rust
    /// use hoprd_keypair::key_pair::HoprKeys;
    ///
    /// let priv_keys = "0x56b29cefcdf576eea306ba2fd5f32e651c09e0abbc018c47bdc6ef44f6b7506f1050f95137770478f50b456267f761f1b8b341a13da68bc32e5c96984fcd52ae";
    /// assert!(HoprKeys::try_from(priv_keys).is_ok());
    /// ```
    fn try_from(s: &str) -> std::result::Result<Self, Self::Error> {
        let maybe_priv_key = match s.strip_prefix("0x") {
            Some(priv_without_prefix) => priv_without_prefix,
            None => s,
        };

        if maybe_priv_key.len() != 2 * (PACKET_KEY_LENGTH + CHAIN_KEY_LENGTH) {
            return Err(KeyPairError::InvalidPrivateKeySize {
                actual: maybe_priv_key.len(),
                expected: 2 * (PACKET_KEY_LENGTH + CHAIN_KEY_LENGTH),
            });
        }

        let mut priv_key_raw = [0u8; PACKET_KEY_LENGTH + CHAIN_KEY_LENGTH];
        hex::decode_to_slice(maybe_priv_key, &mut priv_key_raw[..])?;

        priv_key_raw.try_into()
    }
}

impl TryFrom<[u8; PACKET_KEY_LENGTH + CHAIN_KEY_LENGTH]> for HoprKeys {
    type Error = KeyPairError;
    /// Deserializes HoprKeys from binary string
    ///
    /// ```rust
    /// use hoprd_keypair::key_pair::HoprKeys;
    ///
    /// let priv_keys = [
    ///     0x56,0xb2,0x9c,0xef,0xcd,0xf5,0x76,0xee,0xa3,0x06,0xba,0x2f,0xd5,0xf3,0x2e,0x65,
    ///     0x1c,0x09,0xe0,0xab,0xbc,0x01,0x8c,0x47,0xbd,0xc6,0xef,0x44,0xf6,0xb7,0x50,0x6f,
    ///     0x10,0x50,0xf9,0x51,0x37,0x77,0x04,0x78,0xf5,0x0b,0x45,0x62,0x67,0xf7,0x61,0xf1,
    ///     0xb8,0xb3,0x41,0xa1,0x3d,0xa6,0x8b,0xc3,0x2e,0x5c,0x96,0x98,0x4f,0xcd,0x52,0xae
    /// ];
    /// assert!(HoprKeys::try_from(priv_keys).is_ok());
    /// ```
    fn try_from(value: [u8; CHAIN_KEY_LENGTH + PACKET_KEY_LENGTH]) -> std::result::Result<Self, Self::Error> {
        let mut packet_key = [0u8; PACKET_KEY_LENGTH];
        packet_key.copy_from_slice(&value[0..32]);
        let mut chain_key = [0u8; CHAIN_KEY_LENGTH];
        chain_key.copy_from_slice(&value[32..64]);

        (packet_key, chain_key).try_into()
    }
}

impl TryFrom<([u8; PACKET_KEY_LENGTH], [u8; CHAIN_KEY_LENGTH])> for HoprKeys {
    type Error = KeyPairError;

    /// Deserializes HoprKeys from tuple of two binary private keys
    ///
    /// ```rust
    /// use hoprd_keypair::key_pair::HoprKeys;
    ///
    /// let priv_keys = (
    /// [
    ///     0x56,0xb2,0x9c,0xef,0xcd,0xf5,0x76,0xee,0xa3,0x06,0xba,0x2f,0xd5,0xf3,0x2e,0x65,
    ///     0x1c,0x09,0xe0,0xab,0xbc,0x01,0x8c,0x47,0xbd,0xc6,0xef,0x44,0xf6,0xb7,0x50,0x6f,
    /// ], [
    ///     0x10,0x50,0xf9,0x51,0x37,0x77,0x04,0x78,0xf5,0x0b,0x45,0x62,0x67,0xf7,0x61,0xf1,
    ///     0xb8,0xb3,0x41,0xa1,0x3d,0xa6,0x8b,0xc3,0x2e,0x5c,0x96,0x98,0x4f,0xcd,0x52,0xae
    /// ]);
    /// assert!(HoprKeys::try_from(priv_keys).is_ok());
    /// ```
    fn try_from(value: ([u8; PACKET_KEY_LENGTH], [u8; CHAIN_KEY_LENGTH])) -> std::result::Result<Self, Self::Error> {
        Ok(HoprKeys {
            packet_key: (value.0, OffchainPublicKey::from_privkey(&value.0[..])?),
            chain_key: (value.1, PublicKey::from_privkey(&value.1[..])?),
            id: Uuid::new_v4(),
        })
    }
}

impl PartialEq for HoprKeys {
    fn eq(&self, other: &Self) -> bool {
        self.packet_key.eq(&other.packet_key) && self.chain_key.eq(&other.chain_key)
    }
}

impl HoprKeys {
    pub fn new() -> Self {
        Self {
            packet_key: OffchainPublicKey::random_keypair(),
            chain_key: PublicKey::random_keypair(),
            id: Uuid::new_v4(),
        }
    }

    pub fn init(opts: IdentityOptions) -> Result<Self> {
        let exists = metadata(&opts.id_path).is_ok();

        if !exists && opts.private_key.is_some() {
            let keys = if let Some(private_key) = opts.private_key {
                if private_key.len() != PACKET_KEY_LENGTH + CHAIN_KEY_LENGTH {
                    return Err(KeyPairError::InvalidPrivateKeySize {
                        actual: private_key.len(),
                        expected: 64,
                    });
                }

                let mut priv_keys = [0u8; PACKET_KEY_LENGTH + CHAIN_KEY_LENGTH];
                priv_keys.clone_from_slice(&private_key);

                priv_keys.try_into()?
            } else {
                HoprKeys::new()
            };
            keys.write_eth_keystore(
                &opts.id_path,
                &opts.password,
                if let Some(true) = opts.use_weak_crypto {
                    true
                } else {
                    false
                },
            )?;

            return Ok(keys);
        }

        if exists {
            match HoprKeys::read_eth_keystore(&opts.id_path, &opts.password) {
                Ok((keys, needs_migration)) => {
                    if needs_migration {
                        keys.write_eth_keystore(
                            &opts.id_path,
                            &opts.password,
                            if let Some(true) = opts.use_weak_crypto {
                                true
                            } else {
                                false
                            },
                        )?
                    }
                    return Ok(keys);
                }
                Err(e) => {
                    error!("{}", e.to_string());
                }
            }
        }

        if opts.initialize {
            let keys = HoprKeys::new();
            keys.write_eth_keystore(
                &opts.id_path,
                &opts.password,
                if let Some(true) = opts.use_weak_crypto {
                    true
                } else {
                    false
                },
            )?;

            return Ok(keys);
        }

        Err(KeyPairError::GeneralError(
            String::from("Key store file does not exist or could not decrypt it. ")
                + "Maybe using the wrong '--password'? "
                + "Otherwise try again with '--initialize' to overwrite the existing key store. "
                + "THIS WILL DESTROY THE PREVIOUS KEY",
        ))
    }

    /// Reads a keystore file using custom FS operations
    ///
    /// Highly inspired by https://github.com/roynalnaruto/eth-keystore-rs
    pub fn read_eth_keystore(path: &str, password: &str) -> Result<(Self, bool)> {
        let json_string = read_to_string(path)?;
        let keystore: EthKeystore = from_json_string(&json_string)?;

        let key = match keystore.crypto.kdfparams {
            KdfparamsType::Scrypt { dklen, n, p, r, salt } => {
                let mut key = vec![0u8; dklen as usize];
                let log_n = (n as f32).log2() as u8;
                let scrypt_params = ScryptParams::new(log_n, r, p, dklen.into())
                    .map_err(|err| KeyPairError::KeyDerivationError { err: err.to_string() })?;
                scrypt(password.as_ref(), &salt, &scrypt_params, key.as_mut_slice())
                    .map_err(|err| KeyPairError::KeyDerivationError { err: err.to_string() })?;
                key
            }
            _ => panic!("HOPR only supports scrypt"),
        };

        // Derive the MAC from the derived key and ciphertext.
        let derived_mac = Keccak256::new()
            .chain(&key[16..32])
            .chain(&keystore.crypto.ciphertext)
            .finalize();

        if derived_mac.as_slice() != keystore.crypto.mac.as_slice() {
            return Err(KeyPairError::MacMismatch);
        }

        // Decrypt the private key bytes using AES-128-CTR
        let decryptor = Aes128Ctr::new(&key[..16], &keystore.crypto.cipherparams.iv[..16]).expect("invalid length");

        let mut pk = keystore.crypto.ciphertext;

        match pk.len() {
            32 => {
                decryptor.apply_keystream(&mut pk);

                let mut packet_key = [0u8; PACKET_KEY_LENGTH];
                getrandom(&mut packet_key)?;

                let mut chain_key = [0u8; 32];
                chain_key.clone_from_slice(&pk.as_slice()[0..32]);

                Ok((
                    HoprKeys {
                        packet_key: (packet_key, OffchainPublicKey::from_privkey(&packet_key[..]).unwrap()),
                        chain_key: (chain_key, PublicKey::from_privkey(&chain_key[..]).unwrap()),
                        id: keystore.id,
                    },
                    true,
                ))
            }
            172 => {
                decryptor.apply_keystream(&mut pk);

                let private_keys = serde_json::from_slice::<PrivateKeys>(&pk)?;

                if private_keys.packet_key.len() != PACKET_KEY_LENGTH {
                    return Err(KeyPairError::InvalidEncryptedKeyLength {
                        actual: private_keys.packet_key.len(),
                        expected: PACKET_KEY_LENGTH,
                    });
                }

                if private_keys.chain_key.len() != CHAIN_KEY_LENGTH {
                    return Err(KeyPairError::InvalidEncryptedKeyLength {
                        actual: private_keys.chain_key.len(),
                        expected: CHAIN_KEY_LENGTH,
                    });
                }

                let mut packet_key = [0u8; PACKET_KEY_LENGTH];
                packet_key.clone_from_slice(private_keys.packet_key.as_slice());

                let mut chain_key = [0u8; CHAIN_KEY_LENGTH];
                chain_key.clone_from_slice(private_keys.chain_key.as_slice());

                Ok((
                    HoprKeys {
                        packet_key: (packet_key, OffchainPublicKey::from_privkey(&packet_key[..]).unwrap()),
                        // TODO: change this to off-chain privKey
                        chain_key: (chain_key, PublicKey::from_privkey(&chain_key[..]).unwrap()),
                        id: keystore.id,
                    },
                    false,
                ))
            }
            _ => {
                return Err(KeyPairError::InvalidEncryptedKeyLength {
                    actual: pk.len(),
                    expected: PACKET_KEY_LENGTH + CHAIN_KEY_LENGTH,
                });
            }
        }
    }

    /// Writes a keystore file using custom FS operation and custom entropy source
    ///
    /// Highly inspired by https://github.com/roynalnaruto/eth-keystore-rs
    pub fn write_eth_keystore(&self, path: &str, password: &str, use_weak_crypto: bool) -> Result<()> {
        // Generate a random salt.
        let mut salt = [0u8; HOPR_KEY_SIZE];

        getrandom(&mut salt)?;

        // Derive the key.
        let mut key = [0u8; HOPR_KDF_PARAMS_DKLEN as usize];
        let scrypt_params = ScryptParams::new(
            if use_weak_crypto { 1 } else { HOPR_KDF_PARAMS_LOG_N },
            HOPR_KDF_PARAMS_R,
            HOPR_KDF_PARAMS_P,
            HOPR_KDF_PARAMS_DKLEN.into(),
        )
        .map_err(|e| KeyPairError::KeyDerivationError { err: e.to_string() })?;

        scrypt(password.as_ref(), &salt, &scrypt_params, key.as_mut_slice())
            .map_err(|e| KeyPairError::KeyDerivationError { err: e.to_string() })?;

        // Encrypt the private key using AES-128-CTR.
        let mut iv = [0u8; HOPR_IV_SIZE];
        getrandom(&mut iv)?;

        let encryptor = Aes128Ctr::new(&key[..16], &iv[..16]).expect("invalid length");

        let private_keys = PrivateKeys {
            chain_key: self.chain_key.0.into(),
            packet_key: self.packet_key.0.into(),
            version: VERSION,
        };

        let mut ciphertext = serde_json::to_vec(&private_keys)?;
        encryptor.apply_keystream(&mut ciphertext);

        // Calculate the MAC.
        let mac = Keccak256::new().chain(&key[16..32]).chain(&ciphertext).finalize();

        // Construct and serialize the encrypted JSON keystore.
        let keystore = EthKeystore {
            id: self.id,
            version: 3,
            crypto: CryptoJson {
                cipher: String::from(HOPR_CIPHER),
                cipherparams: CipherparamsJson { iv: iv.to_vec() },
                ciphertext,
                kdf: KdfType::Scrypt,
                kdfparams: KdfparamsType::Scrypt {
                    dklen: HOPR_KDF_PARAMS_DKLEN,
                    n: 2u32.pow(if use_weak_crypto { 1 } else { HOPR_KDF_PARAMS_LOG_N } as u32),
                    p: HOPR_KDF_PARAMS_P,
                    r: HOPR_KDF_PARAMS_R,
                    salt: salt.to_vec(),
                },
                mac: mac.to_vec(),
            },
        };

        let serialized = to_json_string(&keystore)?;

        write(path, serialized).map_err(|e| e.into())
    }
}

impl Debug for HoprKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HoprKeys")
            .field(
                "packet_key",
                &format_args!("(priv_key: <REDACTED>, pub_key: {}", self.packet_key.1.to_hex()),
            )
            .field(
                "chain_key",
                &format_args!("(priv_key: <REDACTED>, pub_key: {}", self.chain_key.1.to_hex(false)),
            )
            .finish()
    }
}

#[cfg(feature = "wasm")]
pub mod wasm {
    use js_sys::{Promise, Uint8Array};
    use utils_types::traits::PeerIdLike;
    use wasm_bindgen::prelude::*;

    use super::IdentityOptions;

    #[wasm_bindgen(module = "@libp2p/peer-id")]
    extern "C" {
        #[wasm_bindgen]
        pub type JsPeerId;

        #[wasm_bindgen(js_name = "peerIdFromKeys")]
        pub fn peer_id_from_keys(pub_key: Box<[u8]>, priv_key: Box<[u8]>) -> Promise;
    }

    #[wasm_bindgen]
    pub struct HoprKeys {
        w: super::HoprKeys,
    }

    #[wasm_bindgen]
    impl HoprKeys {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Self {
            let keys = super::HoprKeys::new();
            Self { w: keys }
        }

        #[wasm_bindgen]
        pub fn init(identity_options: IdentityOptions) -> Result<HoprKeys, JsValue> {
            Ok(Self {
                w: super::HoprKeys::init(identity_options).map_err(|e| JsValue::from(e.to_string()))?,
            })
        }

        #[wasm_bindgen(getter, js_name = "packetKeyPeerId")]
        pub fn get_packet_key_peer_id(&self) -> Promise {
            let mut sliced = [0u8; 36];
            sliced.copy_from_slice(&self.w.packet_key.1.to_peerid().to_bytes()[2..]);
            peer_id_from_keys(Box::new(sliced), Box::new(self.w.packet_key.0))
        }

        #[wasm_bindgen(getter, js_name = "chainKeyPeerId")]
        pub fn get_chain_key_peer_id(&self) -> Promise {
            let mut sliced = [0u8; 37];
            sliced.copy_from_slice(&self.w.chain_key.1.to_peerid().to_bytes()[2..]);

            peer_id_from_keys(Box::new(sliced), Box::new(self.w.chain_key.0))
        }

        #[wasm_bindgen(getter, js_name = "chainKeyPrivKey")]
        pub fn get_chain_key_priv_key(&self) -> Uint8Array {
            Uint8Array::from(&self.w.chain_key.0[..])
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::HoprKeys;
    use tempfile::tempdir;
    use utils_types::traits::PeerIdLike;

    const DEFAULT_PASSWORD: &str = "dummy password for unit testing";

    #[test]
    fn create_keys() {
        println!("{:?}", HoprKeys::new())
    }

    #[test]
    fn store_keys_and_read_them() {
        let tmp = tempdir().unwrap();

        let identity_dir = tmp.path().join("hopr-unit-test-identity");

        let keys = HoprKeys::new();

        keys.write_eth_keystore(identity_dir.to_str().unwrap(), DEFAULT_PASSWORD, true)
            .unwrap();

        let (deserialized, needs_migration) =
            HoprKeys::read_eth_keystore(identity_dir.to_str().unwrap(), DEFAULT_PASSWORD).unwrap();

        assert!(!needs_migration);
        assert_eq!(deserialized, keys);
    }

    #[test]
    fn test_migration() {
        let tmp = tempdir().unwrap();

        let identity_dir = tmp.path().join("hopr-unit-test-identity");

        let old_keystore_file = r#"{"id":"8e5fe142-6ef9-4fbb-aae8-5de32b680e31","version":3,"crypto":{"cipher":"aes-128-ctr","cipherparams":{"iv":"04141354edb9dfb0c65e6905a3a0b9dd"},"ciphertext":"74f12f72cf2d3d73ff09f783cb9b57995b3808f7d3f71aa1fa1968696aedfbdd","kdf":"scrypt","kdfparams":{"salt":"f5e3f04eaa0c9efffcb5168c6735d7e1fe4d96f48a636c4f00107e7c34722f45","n":1,"dklen":32,"p":1,"r":8},"mac":"d0daf0e5d14a2841f0f7221014d805addfb7609d85329d4c6424a098e50b6fbe"}}"#;

        fs::write(identity_dir.to_str().unwrap(), old_keystore_file.as_bytes()).unwrap();

        let (deserialized, needs_migration) =
            HoprKeys::read_eth_keystore(identity_dir.to_str().unwrap(), "local").unwrap();

        assert!(needs_migration);
        assert_eq!(
            deserialized.chain_key.1.to_peerid_str(),
            "16Uiu2HAm8WFpakjrdWauUKq2hb5bejivnbtFAumVv9KHKN5AvXXK"
        );
    }

    #[test]
    fn test_auto_migration() {
        let tmp = tempdir().unwrap();
        let identity_dir = tmp.path().join("hopr-unit-test-identity");

        let old_keystore_file = r#"{"id":"8e5fe142-6ef9-4fbb-aae8-5de32b680e31","version":3,"crypto":{"cipher":"aes-128-ctr","cipherparams":{"iv":"04141354edb9dfb0c65e6905a3a0b9dd"},"ciphertext":"74f12f72cf2d3d73ff09f783cb9b57995b3808f7d3f71aa1fa1968696aedfbdd","kdf":"scrypt","kdfparams":{"salt":"f5e3f04eaa0c9efffcb5168c6735d7e1fe4d96f48a636c4f00107e7c34722f45","n":1,"dklen":32,"p":1,"r":8},"mac":"d0daf0e5d14a2841f0f7221014d805addfb7609d85329d4c6424a098e50b6fbe"}}"#;
        fs::write(identity_dir.to_str().unwrap(), old_keystore_file.as_bytes()).unwrap();

        assert!(HoprKeys::init(super::IdentityOptions {
            initialize: false,
            id_path: identity_dir.to_str().unwrap().into(),
            password: "local".into(),
            use_weak_crypto: None,
            private_key: None,
        })
        .is_ok());

        let (deserialized, needs_migration) =
            HoprKeys::read_eth_keystore(identity_dir.to_str().unwrap(), "local").unwrap();

        assert!(!needs_migration);
        assert_eq!(
            deserialized.chain_key.1.to_peerid_str(),
            "16Uiu2HAm8WFpakjrdWauUKq2hb5bejivnbtFAumVv9KHKN5AvXXK"
        );
    }
}
