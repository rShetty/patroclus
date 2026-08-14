use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};

use crate::errors::{PatroclusError, Result};

pub struct KeyPair {
    pub private_pem: String,
    pub public_pem: String,
}

impl KeyPair {
    pub fn generate() -> Result<Self> {
        let mut rng = rand::thread_rng();
        let priv_key = rsa::RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|e| PatroclusError::Crypto(format!("key generation failed: {}", e)))?;
        let private_pem = priv_key
            .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .map_err(|e| PatroclusError::Crypto(format!("private key encoding failed: {}", e)))?
            .to_string();
        let public_pem = priv_key
            .to_public_key()
            .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .map_err(|e| PatroclusError::Crypto(format!("public key encoding failed: {}", e)))?;
        Ok(KeyPair {
            private_pem,
            public_pem,
        })
    }

    pub fn from_files(private_key_path: &str, public_key_path: &str) -> Result<Self> {
        let private_pem = std::fs::read_to_string(private_key_path)?;
        let public_pem = std::fs::read_to_string(public_key_path)?;
        Ok(KeyPair {
            private_pem,
            public_pem,
        })
    }

    pub fn load_or_generate(private_key_path: &str, public_key_path: &str) -> Result<Self> {
        match Self::from_files(private_key_path, public_key_path) {
            Ok(kp) => Ok(kp),
            Err(_) => {
                tracing::warn!(
                    "No keys found at {} / {}, generating ephemeral keypair (not for production)",
                    private_key_path,
                    public_key_path
                );
                KeyPair::generate()
            }
        }
    }
}
