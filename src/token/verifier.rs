use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

use crate::errors::{PatroclusError, Result};
use crate::token::AgentClaims;

pub struct TokenVerifier {
    decoding_key: DecodingKey,
    issuer: String,
    jti_store: parking_lot::RwLock<std::collections::HashSet<String>>,
}

impl TokenVerifier {
    pub fn ephemeral(issuer: &str) -> Result<Self> {
        use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};
        let mut rng = rand::thread_rng();
        let priv_key = rsa::RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|e| PatroclusError::Crypto(format!("key generation failed: {}", e)))?;
        let pub_pem = priv_key
            .to_public_key()
            .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .map_err(|e| PatroclusError::Crypto(format!("public key encoding failed: {}", e)))?;
        Self::from_pem(pub_pem.as_str(), issuer)
    }

    pub fn from_pem(pem: &str, issuer: &str) -> Result<Self> {
        let decoding_key = DecodingKey::from_rsa_pem(pem.as_bytes())
            .map_err(|e| PatroclusError::Crypto(format!("failed to load public key: {}", e)))?;
        Ok(TokenVerifier {
            decoding_key,
            issuer: issuer.to_string(),
            jti_store: parking_lot::RwLock::new(std::collections::HashSet::new()),
        })
    }

    pub fn verify(&self, token: &str, expected_audience: Option<&str>) -> Result<AgentClaims> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.validate_aud = expected_audience.is_some();
        if let Some(aud) = expected_audience {
            validation.set_audience(&[aud]);
        }

        let token_data =
            decode::<AgentClaims>(token, &self.decoding_key, &validation).map_err(|e| {
                match e.kind() {
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                        PatroclusError::ExpiredToken
                    }
                    _ => PatroclusError::InvalidToken(e.to_string()),
                }
            })?;

        let now = Utc::now();
        if token_data.claims.exp < now.timestamp() {
            return Err(PatroclusError::ExpiredToken);
        }

        {
            let store = self.jti_store.read();
            if store.contains(&token_data.claims.jti) {
                return Err(PatroclusError::RevokedToken(token_data.claims.jti));
            }
        }

        Ok(token_data.claims)
    }

    pub fn revoke(&self, jti: &str) {
        let mut store = self.jti_store.write();
        store.insert(jti.to_string());
    }

    pub fn is_revoked(&self, jti: &str) -> bool {
        let store = self.jti_store.read();
        store.contains(jti)
    }
}
