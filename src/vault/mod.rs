use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::errors::{PatroclusError, Result};

pub mod providers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultCredential {
    pub id: Uuid,
    pub principal_id: Uuid,
    pub provider: String,
    pub encrypted_token: Vec<u8>,
    pub nonce: Vec<u8>,
    pub encryption_key_id: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreCredentialRequest {
    pub principal_id: Uuid,
    pub provider: String,
    pub refresh_token: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendCredentialRequest {
    pub principal_id: Uuid,
    pub provider: String,
    pub requested_scopes: Vec<String>,
    pub agent_token_jti: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendCredentialResponse {
    pub provider: String,
    pub access_token: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub vended_for_jti: String,
}

pub struct Vault {
    encryption_key: [u8; 32],
    key_id: String,
}

impl Vault {
    pub fn new(key_material: &[u8]) -> Result<Self> {
        let mut hasher = Sha256::new();
        hasher.update(key_material);
        let mut key = [0u8; 32];
        key.copy_from_slice(&hasher.finalize());
        Ok(Vault {
            encryption_key: key,
            key_id: hex::encode(&key[..8]),
        })
    }

    pub fn from_file(path: &str) -> Result<Self> {
        let key_material = std::fs::read_to_string(path)
            .map_err(|e| PatroclusError::Vault(format!("failed to read vault key: {}", e)))?;
        Self::new(key_material.as_bytes())
    }

    pub fn generate_key(path: &str) -> Result<()> {
        let mut rng = rand::thread_rng();
        let mut key = [0u8; 32];
        for byte in key.iter_mut() {
            *byte = rand::Rng::r#gen(&mut rng);
        }
        let encoded = hex::encode(key);
        std::fs::write(path, &encoded)?;
        Ok(())
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        let cipher = Aes256Gcm::new(&self.encryption_key.into());

        let mut nonce_bytes = [0u8; 12];
        for byte in nonce_bytes.iter_mut() {
            *byte = rand::Rng::r#gen(&mut rand::thread_rng());
        }
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| PatroclusError::Vault(format!("encryption failed: {}", e)))?;

        Ok((ciphertext, nonce_bytes.to_vec()))
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String> {
        let cipher = Aes256Gcm::new(&self.encryption_key.into());

        let nonce = Nonce::from_slice(nonce);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| PatroclusError::Vault(format!("decryption failed: {}", e)))?;

        String::from_utf8(plaintext)
            .map_err(|e| PatroclusError::Vault(format!("plaintext decode failed: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let vault = Vault::new(b"test-key-material").unwrap();
        let plaintext = "ghp_abcdef1234567890";
        let (ciphertext, nonce) = vault.encrypt(plaintext).unwrap();
        assert_ne!(ciphertext, plaintext.as_bytes());
        let decrypted = vault.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces_each_encrypt() {
        let vault = Vault::new(b"test-key").unwrap();
        let (_, nonce1) = vault.encrypt("hello").unwrap();
        let (_, nonce2) = vault.encrypt("hello").unwrap();
        assert_ne!(nonce1, nonce2);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let vault1 = Vault::new(b"key1").unwrap();
        let vault2 = Vault::new(b"key2").unwrap();
        let (ciphertext, nonce) = vault1.encrypt("secret").unwrap();
        assert!(vault2.decrypt(&ciphertext, &nonce).is_err());
    }
}
